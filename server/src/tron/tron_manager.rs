use kameo::{Actor, actor::ActorRef, mailbox::unbounded};
use thiserror::Error;

use crate::tron::tron_block_scanner::TronBlockScanner;

#[derive(Debug, Error, Clone)]
pub enum TronManagerError {
    #[error("启动子服务：{0} 失败,原因: {1}")]
    StartChildError(String, String),
}

pub(crate) struct TronManager;

impl TronManager {
    pub(crate) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
    ) -> anyhow::Result<ActorRef<Self>> {
        let manager = TronManager;
        let actor_ref =
            Actor::spawn_link_with_mailbox(supervisor, manager, unbounded::<Self>()).await;
        actor_ref.wait_for_startup_result().await?;

        Ok(actor_ref)
    }
}

impl Actor for TronManager {
    type Args = Self;

    type Error = TronManagerError;

    async fn on_start(
        args: Self::Args,
        actor_ref: kameo::prelude::ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        let ret = TronBlockScanner::spawn_link(&actor_ref).await;
        let _ = ret.map_err(|e| {
            TronManagerError::StartChildError("Tron Scanner".to_string(), format!("{e:?}"))
        })?;

        Ok(args)
    }
}
