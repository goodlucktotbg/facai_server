use std::time::Duration;

use entities::entities::daili::Entity as DailiEntity;
use entities::entities::daili_group::Entity as DailiGroupEntity;
use entities::entities::fish::Entity as FishEntity;
use entities::entities::fish_browse::Entity as FishBrowseEntity;
use entities::entities::options::Entity as OptionsEntity;

use futures_util::StreamExt;
use kameo::{Actor, actor::ActorRef, mailbox::unbounded, prelude::Message};
use sea_orm::EntityTrait;
use thiserror::Error;
use tracing::{error, info};

use crate::{
    daili::daili_cache, daili_group::daili_group_cache, fish::fish_cache,
    fish_browse::fish_browse_cache, options::options_cache,
};

#[derive(Debug, Error, Clone)]
pub enum DataCacheManageError {}

pub(crate) struct DataCacheManager {
    db_conn: sea_orm::DatabaseConnection,
}

impl DataCacheManager {
    pub(crate) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
    ) -> anyhow::Result<ActorRef<Self>> {
        let db_conn = database::connection::get_connection().await?;
        let dm = DataCacheManager { db_conn };
        let actor_ref =
            kameo::Actor::spawn_link_with_mailbox(supervisor, dm, unbounded::<Self>()).await;
        actor_ref.wait_for_startup_result().await?;

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
        args.reload_data(actor_ref).await;

        Ok(args)
    }
}

impl DataCacheManager {
    async fn reload_data(&self, actor_ref: ActorRef<Self>) {
        info!("now reload data");
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
}

struct ReloadCacheCmd;

impl Message<ReloadCacheCmd> for DataCacheManager {
    type Reply = ();

    async fn handle(
        &mut self,
        _msg: ReloadCacheCmd,
        ctx: &mut kameo::prelude::Context<Self, Self::Reply>,
    ) -> Self::Reply {
        self.reload_data(ctx.actor_ref()).await;
    }
}
