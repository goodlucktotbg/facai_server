use dashmap::DashMap;
use entities::entities::options::Model as OptionsModel;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use tracing::error;

pub(crate) fn clear() {
    OPTIONS.clear();
}

pub(crate) fn cache(data: OptionsModel) {
    let key = match data.name.as_ref() {
        Some(name) => match name.as_str() {
            "0x_permission_address" => "OxPermissionAddress".to_string(),
            "0x_private_key" => "OxPrivateKey".to_string(),
            "permission_address" => match data.value.as_deref() {
                Some(address) => {
                    // 缓存授权地址
                    let addr = address
                        .split("\r\n")
                        .filter_map(|addr| {
                            let new_addr = addr.trim();
                            if new_addr.is_empty() {
                                None
                            } else {
                                Some(new_addr.to_string())
                            }
                        })
                        .collect::<Vec<_>>();
                    let mut write = PERMISSION_ADDRESSES.write();
                    *write = addr;
                    name.clone()
                }
                None => {
                    error!("缓存授权地址出错：未设置授权地址");
                    return;
                }
            },

            _ => name.clone(),
        },
        None => {
            error!("缓存options出错：没有name字段");
            return;
        }
    };

    OPTIONS.insert(key, data);
}

pub(crate) static OPTIONS: Lazy<DashMap<String, OptionsModel>> = Lazy::new(DashMap::new);

pub(crate) static PERMISSION_ADDRESSES: Lazy<RwLock<Vec<String>>> =
    Lazy::new(|| RwLock::new(Vec::new()));
