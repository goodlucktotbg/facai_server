use kameo::{Actor, mailbox::unbounded};
use thiserror::Error;

use crate::bus::event::event_manager::EventManager;
use crate::{
    data_cache_manager::DataCacheManager, fish_browse::fish_browse_manager::FishBrowseManager,
    telegram_bot::telegram_bot_manager::TelegramBotManager, tron::tron_manager::TronManager,
};
use crate::daili::daili_manager::DailiManager;
use crate::fish::fish_manager::FishManager;

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
        EventManager::spawn_link(&actor_ref).await.map_err(|e| {
            ApplicationError::StartServiceError("bus".to_string(), format!("{e:?}"))
        })?;

        let event_manager = EventManager::actor_ref()
            .await
            .map_err(|e| ApplicationError::StartServiceError("bus".to_string(), format!("{e:?}")))?
            .ok_or_else(|| {
                ApplicationError::StartServiceError("bus".to_string(), "未注册".to_string())
            })?;

        DataCacheManager::spawn_link(&actor_ref, event_manager.clone())
            .await
            .map_err(|e| {
                ApplicationError::StartServiceError(
                    "DataCacheManager".to_string(),
                    format!("{e:?}"),
                )
            })?;

        DailiManager::spawn_link(&actor_ref).await.map_err(|e| {
            ApplicationError::StartServiceError("daili manager".to_string(), format!("{e:?}"))
        })?;

        FishManager::spawn_link(&actor_ref).await.map_err(|e| {
            ApplicationError::StartServiceError("fish manager".to_string(), format!("{e:?}"))
        })?;

        let bot = TelegramBotManager::init_bot().map_err(|e| {
            ApplicationError::StartServiceError("init bot instance".to_string(), format!("{e:}"))
        })?;

        FishBrowseManager::spawn_link(&actor_ref, bot.clone())
            .await
            .map_err(|e| {
                ApplicationError::StartServiceError(
                    "Fish Browse Manager".to_string(),
                    format!("{e:?}"),
                )
            })?;

        TronManager::spawn_link(&actor_ref, bot.clone())
            .await
            .map_err(|e| {
                ApplicationError::StartServiceError("Tron Manager".to_string(), format!("{e:?}"))
            })?;

        TelegramBotManager::spawn_link(&actor_ref, bot.clone())
            .await
            .map_err(|e| {
                ApplicationError::StartServiceError("Telegram Bot".to_string(), format!("{e:?}"))
            })?;

        Ok(args)
    }
}
