use chrono::{Local, NaiveDateTime, ParseResult};
use rand::prelude::SliceRandom;
use sha2::{Digest, Sha256};
use teloxide::Bot;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{ParseMode, Recipient};
use tracing::error;

/// 对数据执行两次 sha256（sha256d）
pub fn sha256d(data: &[u8]) -> Vec<u8> {
    let first = Sha256::digest(data);
    let second = Sha256::digest(&first);
    second.to_vec()
}

pub fn random_one<T>(slice: &mut [T]) -> Option<&T> {
    let mut rng = rand::rng();
    let (r, _) = slice.partial_shuffle(&mut rng, 1);
    r.first()
}

pub fn now_date_time_str() -> String {
    let now = Local::now();
    now.format("%Y-%m-%d %H:%M:%S %:z").to_string()
}

pub fn now_data_time_str_without_zone() -> String {
    let now = Local::now();
    now.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn parse_native_date_time(time: &str) -> ParseResult<NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S")
}

pub async fn send_bot_message<C>(bot: &Bot, chat_id: C, text: impl Into<String>, parse_mode: Option<ParseMode>)
where
    C: Into<Recipient>,
{
    let ret =
        if let Some(mode) = parse_mode {
            bot.send_message(chat_id, text).parse_mode(mode).await
        } else {
            bot.send_message(chat_id, text).await
        };
    match ret {
        Ok(_) => {}
        Err(e) => {
            error!("发送消息到机器人失败: {e:?}");
        }
    }
}
