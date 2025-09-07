use config_helper::CONFIG;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::{Compact, Format};

//
// log_level = "DEBUG" #  TRACE DEBUG  INFO  WARN ERROR
pub fn get_log_level() -> tracing::Level {
    match CONFIG.log.level.to_lowercase().as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::DEBUG,
    }
}

pub fn init_env() {
    if std::env::var_os("RUST_LOG").is_none() {
        unsafe {
            std::env::set_var("RUST_LOG", &CONFIG.log.level.to_uppercase());
        }
    }
}

#[cfg(target_os = "windows")]
use time::format_description::well_known::Rfc3339;
#[cfg(target_os = "windows")]
use tracing_subscriber::fmt::time::LocalTime;
#[cfg(target_os = "windows")]
pub fn get_log_format() -> Format<Compact, LocalTime<Rfc3339>> {
    fmt::format()
        .with_level(true) // don't include levels in formatted output
        .with_target(true) // don't include targets
        .with_thread_ids(true)
        // include the thread ID of the current thread
        // .with_thread_names(true)
        // .with_file(true)
        // .with_ansi(true)
        // .with_line_number(true) // include the name of the current thread
        .with_timer(LocalTime::rfc_3339()) // use RFC 3339 timestamps
        .compact()
}

#[cfg(not(target_os = "windows"))]
pub fn get_log_format() -> Format<Compact> {
    fmt::format()
        .with_level(true)
        .with_target(true)
        .with_thread_ids(true)
        .compact()
}
