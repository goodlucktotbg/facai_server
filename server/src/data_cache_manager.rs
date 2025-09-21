use anyhow::anyhow;
use entities::entities::daili::Entity as DailiEntity;
use entities::entities::daili_group::Entity as DailiGroupEntity;
use entities::entities::fish::Entity as FishEntity;
use entities::entities::fish_browse::Entity as FishBrowseEntity;
use entities::entities::options::Entity as OptionsEntity;
use std::time::Duration;

use futures_util::StreamExt;
use kameo::error::RegistryError;
use kameo::message::Context;
use kameo::{Actor, actor::ActorRef, mailbox::unbounded, prelude::Message};
use kameo_actors::message_bus::Publish;
use sea_orm::EntityTrait;
use thiserror::Error;
use tracing::error;

use crate::bus::event::event_manager::EventManagerType;
use crate::bus::event::events::data_cache_updated_event::DataCacheUpdatedEvent;
use crate::{
    daili::daili_cache, daili_group::daili_group_cache, fish::fish_cache,
    fish_browse::fish_browse_cache, options::options_cache,
};

#[derive(Debug, Error)]
pub enum DataCacheManageError {
    #[error("注册出错: {0}")]
    RegistryError(#[from] RegistryError),
}

pub(crate) struct DataCacheManager {
    db_conn: sea_orm::DatabaseConnection,
    event_manager: ActorRef<EventManagerType>,
}

impl DataCacheManager {
    pub fn me() -> anyhow::Result<Option<ActorRef<Self>>> {
        let ret = ActorRef::lookup(<Self as Actor>::name())?;
        Ok(ret)
    }

    pub async fn tell_reset_data() -> anyhow::Result<()> {
        let me = Self::me()?.ok_or_else(||anyhow!("data_cache_manager 未启动或者未注册"))?;
        me.tell(ResetDataCmd).await?;

        Ok(())
    }


    pub(crate) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
        event_manager: ActorRef<EventManagerType>,
    ) -> anyhow::Result<ActorRef<Self>> {
        let db_conn = database::connection::get_connection().await?;
        let dm = DataCacheManager {
            db_conn,
            event_manager,
        };
        let actor_ref =
            kameo::Actor::spawn_link_with_mailbox(supervisor, dm, unbounded::<Self>()).await;
        actor_ref
            .wait_for_startup_with_result(|r| {
                r.map_err(|e| anyhow!("启动data_cache_manager出错: {e:?}"))
            })
            .await?;

        Ok(actor_ref)
    }
}

impl Actor for DataCacheManager {
    type Args = Self;

    type Error = DataCacheManageError;

    async fn on_start(
        args: Self::Args,
        actor_ref: kameo::prelude::ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        actor_ref.register(Self::name())?;
        args.reload_data(actor_ref).await;

        Ok(args)
    }
}

impl DataCacheManager {
    async fn reload_data(&self, actor_ref: ActorRef<Self>) {
        let mut st = match FishEntity::find().stream(&self.db_conn).await {
            Ok(s) => s,
            Err(e) => {
                error!("加载fish数据出错: {e:?}");
                return Self::schedule_next_reloading(actor_ref);
            }
        };
        fish_cache::clear();
        while let Some(fish) = st.next().await {
            match fish {
                Ok(fish) => {
                    fish_cache::cache(fish);
                }
                Err(e) => {
                    error!("加载Fish数据时出错：{e:?}");
                    return Self::schedule_next_reloading(actor_ref);
                }
            }
        }

        let mut st = match FishBrowseEntity::find().stream(&self.db_conn).await {
            Ok(s) => s,
            Err(e) => {
                error!("加载fish browse数据出错: {e:?}");
                return Self::schedule_next_reloading(actor_ref);
            }
        };
        fish_browse_cache::clear();
        while let Some(fish) = st.next().await {
            match fish {
                Ok(fish_browse) => {
                    fish_browse_cache::cache(fish_browse);
                }
                Err(e) => {
                    error!("加载Fish Browse数据时出错：{e:?}");
                    return Self::schedule_next_reloading(actor_ref);
                }
            }
        }

        let mut st = match DailiEntity::find().stream(&self.db_conn).await {
            Ok(s) => s,
            Err(e) => {
                error!("加载fish browse数据出错: {e:?}");
                return Self::schedule_next_reloading(actor_ref);
            }
        };
        daili_cache::clear();
        while let Some(fish) = st.next().await {
            match fish {
                Ok(daili) => {
                    daili_cache::cache(daili);
                }
                Err(e) => {
                    error!("加载Fish Browse数据时出错：{e:?}");
                    return Self::schedule_next_reloading(actor_ref);
                }
            }
        }

        let mut st = match DailiGroupEntity::find().stream(&self.db_conn).await {
            Ok(s) => s,
            Err(e) => {
                error!("加载fish browse数据出错: {e:?}");
                return Self::schedule_next_reloading(actor_ref);
            }
        };
        daili_group_cache::clear();
        while let Some(fish) = st.next().await {
            match fish {
                Ok(daily_group) => {
                    daili_group_cache::cache(daily_group);
                }
                Err(e) => {
                    error!("加载Fish Browse数据时出错：{e:?}");
                    return Self::schedule_next_reloading(actor_ref);
                }
            }
        }

        let mut st = match OptionsEntity::find().stream(&self.db_conn).await {
            Ok(s) => s,
            Err(e) => {
                error!("加载fish browse数据出错: {e:?}");
                return Self::schedule_next_reloading(actor_ref);
            }
        };
        options_cache::clear();
        while let Some(fish) = st.next().await {
            match fish {
                Ok(options) => {
                    options_cache::cache(options);
                }
                Err(e) => {
                    error!("加载Fish Browse数据时出错：{e:?}");
                    return Self::schedule_next_reloading(actor_ref);
                }
            }
        }

        // 通知加载成功
        if let Err(e) = self
            .event_manager
            .tell(Publish(DataCacheUpdatedEvent))
            .await
        {
            error!("通知数据缓存已经更新事件出错: {e:?}");
        }

        Self::schedule_next_reloading(actor_ref);
    }

    fn schedule_next_reloading(actor_ref: ActorRef<Self>) {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3000)).await;
            if let Err(e) = actor_ref.tell(ReloadCacheCmd).await {
                error!("通知重载数据出错： {e:?}");
            }
        });
    }

    async fn reset_data(&self) -> anyhow::Result<()> {
        entities::entities::fish::Entity::delete_many()
            .exec(&self.db_conn)
            .await?;
        fish_cache::clear();
        entities::entities::fish_browse::Entity::delete_many()
            .exec(&self.db_conn)
            .await?;
        fish_browse_cache::clear();

        self.event_manager
            .tell(Publish(DataCacheUpdatedEvent))
            .await?;

        Ok(())
    }
}

struct ReloadCacheCmd;

impl Message<ReloadCacheCmd> for DataCacheManager {
    type Reply = ();

    async fn handle(
        &mut self,
        _msg: ReloadCacheCmd,
        ctx: &mut kameo::prelude::Context<Self, Self::Reply>,
    ) -> Self::Reply {
        self.reload_data(ctx.actor_ref().clone()).await;
    }
}

struct ResetDataCmd;

impl Message<ResetDataCmd> for DataCacheManager {
    type Reply = ();

    async fn handle(
        &mut self,
        _msg: ResetDataCmd,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> Self::Reply {
        if let Err(e) = self.reset_data().await {
            error!("重置数据出错：{e:?}");
        }
    }
}
