use anyhow::{Ok, anyhow};
use kameo::{Actor, actor::ActorRef, mailbox::unbounded};
use teloxide::Bot;

use crate::fish_browse::fish_browse_broad_caster::FishBrowseBroadCaster;

// #[derive(Debug, Error)]
// pub enum FishBrowseManagerError {
//     #[error("Fish browse manager start faild: {0}")]
//     StartError(String),

// }

impl FishBrowseManager {
    pub(crate) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
        bot: Bot,
    ) -> anyhow::Result<ActorRef<Self>> {
        let manager = FishBrowseManager { bot };
        let actor_ref =
            Actor::spawn_link_with_mailbox(supervisor, manager, unbounded::<Self>()).await;
        actor_ref
            .wait_for_startup_with_result(|r| r.map_err(|e| anyhow!("{e:?}")))
            .await?;

        Ok(actor_ref)
    }
}

pub(crate) struct FishBrowseManager {
    bot: Bot,
}

impl Actor for FishBrowseManager {
    type Args = Self;

    type Error = anyhow::Error;

    async fn on_start(
        args: Self::Args,
        actor_ref: kameo::prelude::ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        FishBrowseBroadCaster::spawn_link(&actor_ref, args.bot.clone()).await?;

        Ok(args)
    }
}
