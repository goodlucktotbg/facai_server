use crate::daili::daili_cache;
use entities::entities::daili::ActiveModel as DailiActiveModel;
use kameo::Actor;
use kameo::actor::ActorRef;
use kameo::error::RegistryError;
use kameo::mailbox::unbounded;
use kameo::message::{Context, Message};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection};
use thiserror::Error;
use tracing::error;
use crate::utils::common::now_data_time_str_without_zone;

const MIN_UNIQUE_ID: u64 = 100_000_000;
const MAX_UNIQUE_ID: u64 = 1_000_000_000;
pub const DEFAULT_THRESHOLD: i32 = 10_000_000;

#[derive(Error, Debug)]
pub enum DailiManagerError {
    #[error("注册进入DailiManager出错: {0}")]
    RegistryError(#[from] RegistryError),
    #[error("启动出错: {0}")]
    StartError(String),
    #[error("代理不存在")]
    DailiNotExist,
}

impl DailiManager {
    pub(crate) async fn spawn_link(
        supervisor: &ActorRef<impl Actor>,
    ) -> anyhow::Result<ActorRef<Self>> {
        let db_conn = database::connection::get_connection().await?;
        let args = DailiManager { db_conn };
        let ret =
            <DailiManager as Actor>::spawn_link_with_mailbox(supervisor, args, unbounded()).await;
        ret.wait_for_startup_with_result(|r| {
            r.map_err(|e| DailiManagerError::StartError(format!("{e:?}")))
        })
        .await?;
        Ok(ret)
    }

    pub(crate) async fn me() -> Result<Option<ActorRef<Self>>, RegistryError> {
        let ret = ActorRef::lookup(<Self as Actor>::name())?;
        Ok(ret)
    }

    pub async fn create_or_update_with_actor(
        actor_ref: &ActorRef<Self>,
        tg_uid: String,
        tg_group_id: String,
        user_name: String,
        full_name: String,
    ) -> anyhow::Result<String> {
        let cmd = CreateOrUpdateDailiCmd {
            tg_uid,
            tg_group_id,
            user_name,
            full_name,
        };
        let ret = actor_ref.ask(cmd).await?;

        Ok(ret)
    }

    pub async fn update_payment_address_with_actor(
        actor: &ActorRef<Self>,
        unique_id: String,
        payment_address: String,
    ) -> anyhow::Result<()> {
        actor
            .ask(UpdatePaymentAddress {
                unique_id,
                payment_address,
            })
            .await?;
        Ok(())
    }
}

impl DailiManager {
    async fn handle_create_or_update_daili_cmd(
        &self,
        tg_uid: String,
        tg_group_id: String,
        user_name: String,
        full_name: String,
    ) -> anyhow::Result<String> {
        let ret = super::daili_cache::map_by_user_id_group_id(&tg_uid, &tg_group_id, |m| m.id);
        let id = match ret {
            None => {
                // 创建新代理
                let id = self
                    .create_daili(user_name, full_name, tg_uid, tg_group_id)
                    .await?;
                id
            }
            Some(existing) => {
                let unique_id = self.update_daili(existing, user_name, full_name).await?;
                unique_id
            }
        };

        Ok(id)
    }

    async fn create_daili(
        &self,
        user_name: String,
        full_name: String,
        tg_uid: String,
        tg_group_id: String,
    ) -> anyhow::Result<String> {
        let unique_id = Self::generate_unique_id();
        let now = now_data_time_str_without_zone();
        let am = DailiActiveModel {
            unique_id: ActiveValue::Set(Some(unique_id)),
            tguid: ActiveValue::Set(tg_uid),
            username: ActiveValue::Set(Some(user_name)),
            full_name: ActiveValue::Set(Some(full_name)),
            time: ActiveValue::Set(Some(now)),
            groupid: ActiveValue::Set(Some(tg_group_id)),
            threshold: ActiveValue::Set(Some(DEFAULT_THRESHOLD)),
            ..Default::default()
        };
        let ret = am.insert(&self.db_conn).await?;
        let id = ret
            .unique_id
            .clone()
            .unwrap_or_else(|| "异常的unique_id: None".to_string());
        daili_cache::cache(ret);

        Ok(id)
    }

    async fn update_daili(
        &self,
        id: i32,
        user_name: String,
        full_name: String,
    ) -> anyhow::Result<String> {
        let am = DailiActiveModel {
            id: ActiveValue::Set(id),
            // unique_id: Default::default(),
            username: ActiveValue::Set(Some(user_name)),
            full_name: ActiveValue::Set(Some(full_name)),
            ..Default::default()
        };
        let ret = am.update(&self.db_conn).await?;
        let unique_id = ret
            .unique_id
            .clone()
            .unwrap_or_else(|| "异常unique_id为None".to_string());
        daili_cache::cache(ret);

        Ok(unique_id)
    }

    async fn handle_update_payment_address(
        &self,
        unique_id: String,
        payment_address: String,
    ) -> anyhow::Result<()> {
        if let Some(id) = daili_cache::map(&unique_id, |m| m.id) {
            let am = entities::entities::daili::ActiveModel {
                id: ActiveValue::Set(id),
                payment_address: ActiveValue::Set(Some(payment_address)),
                ..Default::default()
            };
            let m = am.update(&self.db_conn).await?;
            daili_cache::cache(m);
            Ok(())
        } else {
            Err(DailiManagerError::DailiNotExist.into())
        }
    }

    fn generate_unique_id() -> String {
        let unique_id = rand::random_range(MIN_UNIQUE_ID..MAX_UNIQUE_ID).to_string();
        if daili_cache::exist_unique_id(&unique_id) {
            Self::generate_unique_id()
        } else {
            unique_id
        }
    }
}

pub struct DailiManager {
    db_conn: DatabaseConnection,
}

impl Actor for DailiManager {
    type Args = Self;
    type Error = DailiManagerError;

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        actor_ref.register(Self::name())?;
        Ok(args)
    }
}

struct CreateOrUpdateDailiCmd {
    tg_uid: String,
    tg_group_id: String,
    user_name: String,
    full_name: String,
}

impl Message<CreateOrUpdateDailiCmd> for DailiManager {
    type Reply = anyhow::Result<String>;

    async fn handle(
        &mut self,
        CreateOrUpdateDailiCmd {
            tg_uid,
            tg_group_id,
            user_name,
            full_name,
        }: CreateOrUpdateDailiCmd,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> Self::Reply {
        let ret = self
            .handle_create_or_update_daili_cmd(tg_uid, tg_group_id, user_name, full_name)
            .await;
        ret
    }
}

struct UpdatePaymentAddress {
    unique_id: String,
    payment_address: String,
}
impl Message<UpdatePaymentAddress> for DailiManager {
    type Reply = anyhow::Result<()>;

    async fn handle(
        &mut self,
        UpdatePaymentAddress {
            unique_id,
            payment_address,
        }: UpdatePaymentAddress,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> Self::Reply {
        self.handle_update_payment_address(unique_id, payment_address).await
    }
}
