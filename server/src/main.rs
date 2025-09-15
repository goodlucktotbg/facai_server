use tracing::info;
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt};

use crate::application::Application;

mod application;
pub mod daili;
pub mod daili_group;
mod data_cache_manager;
mod env;
pub mod fish;
pub mod fish_browse;
pub(crate) mod options;
pub(crate) mod telegram_bot;
pub(crate) mod tron;
pub mod utils;

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let config = &config_helper::CONFIG;

    env::init_env();
    let log_level = env::get_log_level();
    println!("log level: {}", log_level);
    let format = env::get_log_format();
    // 文件输出
    let file_appender = tracing_appender::rolling::hourly(&config.log.dir, &config.log.file);
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 标准控制台输出
    let (std_non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());
    let logger = Registry::default()
        .with(EnvFilter::from_default_env().add_directive(log_level.into()))
        .with(
            fmt::Layer::default()
                .with_writer(std_non_blocking)
                .event_format(format.clone())
                .pretty(),
        )
        .with(
            fmt::Layer::default()
                .with_writer(non_blocking)
                .event_format(format),
        );
    tracing::subscriber::set_global_default(logger).unwrap();

    let is_debug = cfg!(debug_assertions);
    if is_debug {
        info!("当前环境: Debug")
    } else {
        info!("当前环境: Release")
    }

    Application::start().await?;

    // let hex_address = "41a614f803b6fd780986a42c78ec9c7f77e6ded13c";
    // let address = anychain_tron::TronAddress::from_str(hex_address)?;
    // // let address = anychain_tron::TronAddress::from_hex(hex_address)?;
    // let base58_addres = address.to_base58();
    // info!("base58: {base58_addres}");

    Ok(())
}
