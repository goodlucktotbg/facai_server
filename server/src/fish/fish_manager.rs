use kameo::Actor;
use kameo::actor::ActorRef;
use kameo::error::RegistryError;
use kameo::mailbox::unbounded;
use kameo::message::{Context, Message};
use sea_orm::{ActiveModelTrait, DatabaseConnection};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FishManagerError {
    #[error("注册失败: {0}")]
    RegistryError(#[from] RegistryError),
    #[error("启动失败: {0}")]
    StartUpError(String),
}

pub struct FishManager {
    db_conn: DatabaseConnection,
}

impl FishManager {
    pub(crate) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
    ) -> anyhow::Result<ActorRef<Self>> {
        let db_conn = database::connection::get_connection().await?;
        let actor = FishManager { db_conn };
        let actor_ref =
            <FishManager as Actor>::spawn_link_with_mailbox(supervisor, actor, unbounded()).await;
        actor_ref
            .wait_for_startup_with_result(|r| {
                r.map_err(|e| FishManagerError::StartUpError(e.to_string()))
            })
            .await?;

        Ok(actor_ref)
    }

    pub async fn me() -> Result<Option<ActorRef<Self>>, RegistryError> {
        let ret = ActorRef::lookup(<Self as Actor>::name())?;
        Ok(ret)
    }

    pub async fn update_fish_with_actor(
        actor_ref: &ActorRef<Self>,
        active_model: entities::entities::fish::ActiveModel,
    ) -> anyhow::Result<()> {
        actor_ref.ask(UpdateFishCmd{active_model}).await?;
        Ok(())
    }
}

impl FishManager {
    async fn handle_update_fish_cmd(
        &self,
        am: entities::entities::fish::ActiveModel,
    ) -> anyhow::Result<()> {
        let m = am.update(&self.db_conn).await?;
        super::fish_cache::cache(m);

        Ok(())
    }

    // async fn handle_update_fish_cmd(
    //     &self,
    //     am: entities::entities::fish::ActiveModel,
    // ) -> anyhow::Result<()> {
    //     let m = am.update(&self.db_conn).await?;
    //     super::fish_cache::cache(m);
    //
    //     Ok(())
    // }
}

impl Actor for FishManager {
    type Args = Self;
    type Error = FishManagerError;

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        actor_ref.register(Self::name())?;

        Ok(args)
    }
}

struct UpdateFishCmd {
    active_model: entities::entities::fish::ActiveModel,
}

impl Message<UpdateFishCmd> for FishManager {
    type Reply = anyhow::Result<()>;

    async fn handle(
        &mut self,
        UpdateFishCmd { active_model }: UpdateFishCmd,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> Self::Reply {
        self.handle_update_fish_cmd(active_model).await
    }
}
