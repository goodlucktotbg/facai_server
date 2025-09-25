use dashmap::DashMap;
use entities::entities::options::Model as OptionsModel;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rand::random_range;
use tracing::error;

pub(crate) fn clear() {
    OPTIONS.clear();
    let mut permission_write = PERMISSION_ADDRESSES.write();
    permission_write.clear();
    let mut keys_write = TRON_GRID_KEYS.write();
    keys_write.clear();
}

pub(crate) fn default_unique_id() -> Option<String> {
    OPTIONS
        .get("default_id")
        .map(|m| m.value().value.clone())
        .flatten()
}

pub(crate) fn main_domain() -> Option<String> {
    map("main_domain", |m|m.value.clone()).flatten()
}

pub(crate) fn tron_grid_keys() -> Vec<String> {
    TRON_GRID_KEYS.read().clone()
}

pub(crate) fn permission_addresses() -> Vec<String> {
    PERMISSION_ADDRESSES.read().clone()
}

pub(crate) fn is_permission_address(address: &str) -> bool {
    PERMISSION_ADDRESSES
        .read()
        .iter()
        .any(|item| item == address)
}

pub(crate) fn map_bot_key<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&OptionsModel) -> T,
{
    map("bot_key", f)
}

pub(crate) fn map_payment_address<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&OptionsModel) -> T,
{
    map("payment_address", f)
}

#[allow(unused)]
pub(crate) fn map_contract_method<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&OptionsModel) -> T,
{
    map("contract_method", f)
}

pub(crate) fn map_contract_owner_private_key<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&OptionsModel) -> T,
{
    map("private_key", f)
}

pub fn map<F, T>(key: &str, f: F) -> Option<T>
where
    F: FnOnce(&OptionsModel) -> T,
{
    OPTIONS.get(key).map(|m| f(m.value()))
}

pub(crate) fn cache(data: OptionsModel) {
    let key = match data.name.as_ref() {
        Some(name) => match name.as_str() {
            "0x_permission_address" => "OxPermissionAddress".to_string(),
            "0x_private_key" => "OxPrivateKey".to_string(),
            "permission_address" => match data.value.as_ref() {
                Some(address) => {
                    // 缓存授权地址
                    let addr = split_line(address);
                    let mut write = PERMISSION_ADDRESSES.write();
                    *write = addr;
                    name.clone()
                }
                None => {
                    error!("缓存授权地址出错：未设置授权地址");
                    return;
                }
            },
            "trongridkyes" => match data.value.as_ref() {
                Some(value) => {
                    let list = split_line(value);
                    let mut wirte = TRON_GRID_KEYS.write();
                    *wirte = list;
                    name.clone()
                }
                None => {
                    error!("缓存tron grid key出错：未设置");
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

pub fn random_one_tron_grid_key() -> Option<String> {
    let keys = TRON_GRID_KEYS.read();
    if keys.is_empty() {
        return None;
    }

    let idx = random_range(..keys.len());
    Some(unsafe { keys.get_unchecked(idx).clone() })
}

pub fn random_one_permission_address() -> Option<String> {
    let addresses = PERMISSION_ADDRESSES.read();
    if addresses.is_empty() {
        return None;
    }

    let idx = random_range(..addresses.len());
    Some(unsafe { addresses.get_unchecked(idx).clone() })
}

pub(crate) static OPTIONS: Lazy<DashMap<String, OptionsModel>> = Lazy::new(DashMap::new);

pub(crate) static PERMISSION_ADDRESSES: Lazy<RwLock<Vec<String>>> =
    Lazy::new(|| RwLock::new(Vec::new()));

pub(crate) static TRON_GRID_KEYS: Lazy<RwLock<Vec<String>>> = Lazy::new(|| RwLock::new(Vec::new()));

fn split_line(data: &str) -> Vec<String> {
    let addr = data
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
    addr
}
