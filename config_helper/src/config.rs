use serde::Deserialize;

pub static CONFIG: once_cell::sync::Lazy<Config> = once_cell::sync::Lazy::new(init);

const DEFAULT_CONFIG_PATH: &'static str = "./configs/default.toml";
const DEV_CONFIG_PATH: &'static str = "./configs/dev.toml";
const PROD_CONFIG_PATH: &'static str = "./configs/prod.toml";

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub log: LogConfig,
    // pub auth: AuthConfig,
    pub main_database: DatabaseConfig,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ClientConfig {
    pub api_prefix: String,
    pub address: String,
    pub ssl: bool,
    pub content_gzip: bool,
    pub version: String,
    // pub secret: String,
    pub token_expire_in_ms: i64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthConfig {
    pub db_uri: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub min_connections: u32,
    pub max_connections: u32,
    pub connect_timeout: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LogConfig {
    /// `log_level` 日志输出等级
    pub level: String,
    /// `dir` 日志输出文件夹
    pub dir: String,
    /// `file` 日志输出文件名
    pub file: String,
}

fn init() -> Config {
    let is_debug = cfg!(debug_assertions);
    let extra_config_path = if is_debug {
        DEV_CONFIG_PATH
    } else {
        PROD_CONFIG_PATH
    };
    let s = config::Config::builder()
        .add_source(config::File::with_name(DEFAULT_CONFIG_PATH))
        .add_source(config::File::with_name(extra_config_path).required(false))
        .build()
        .unwrap();
    let config: Config = s.try_deserialize().unwrap();
    config
}
