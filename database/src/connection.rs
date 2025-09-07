use std::time::Duration;
use parking_lot::RwLock;
use sea_orm::{DatabaseConnection};
use config_helper::CONFIG;

pub static MAIN_DATABASE_CONNECTION: RwLock<Option<sea_orm::DatabaseConnection>> = RwLock::new(None);


/// 获取数据库连接，如果还没有，则会初始化一下链接
pub async fn get_connection() -> anyhow::Result<DatabaseConnection> {
    // 如果已经初始化好了连接，会直接返回连接
    // 通过引用一个局部作用域，避免编译器认为read锁跨越了异步调用
    {
        let read = MAIN_DATABASE_CONNECTION.read();
        match &*read {
            None => {
            }
            Some(conn) => {
                return Ok(conn.clone())
            }
        }
    }

    // 还未初始化连接，执行初始化操作
    let conn = init_connection().await?;
    Ok(conn)
}

async fn init_connection() -> anyhow::Result<DatabaseConnection> {
    let url = &CONFIG.main_database.url;
    let min = CONFIG.main_database.min_connections;
    let max = CONFIG.main_database.max_connections;
    let connect_time = CONFIG.main_database.connect_timeout;

    let mut options = sea_orm::ConnectOptions::new(url);
    options.min_connections(min).max_connections(max);
    options.connect_timeout(Duration::from_millis(connect_time as u64));

    match sea_orm::Database::connect(options).await {
        Ok(conn) => {
            let mut write = MAIN_DATABASE_CONNECTION.write();
            *write = Some(conn.clone());
            Ok(conn)
        }
        Err(e) => {
            tracing::error!("connect mall server database failed, reason: {e}");
            Err(e.into())
        }
    }
}