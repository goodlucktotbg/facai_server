use kameo::{Actor, mailbox::unbounded};
use thiserror::Error;

use crate::data_cache_manager::DataCacheManager;

#[derive(Debug, Error, Clone)]
pub enum ApplicationError {
    #[error("start service: {0} failed, reason: {1}")]
    StartServiceError(String, String),
}

pub(crate) struct Application;

impl Application {
    pub(crate) async fn start() -> anyhow::Result<()> {
        let actor_ref = kameo::Actor::spawn_with_mailbox(Application, unbounded::<Self>());
        let ret = actor_ref.wait_for_shutdown_result().await?;
        Ok(ret)
    }
}

impl Actor for Application {
    type Args = Self;

    type Error = ApplicationError;

    async fn on_start(
        args: Self::Args,
        actor_ref: kameo::prelude::ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        DataCacheManager::spawn_link(&actor_ref)
            .await
            .map_err(|e| {
                ApplicationError::StartServiceError(
                    "DataCacheManager".to_string(),
                    format!("{e:?}"),
                )
            })?;

        Ok(args)
    }
}
