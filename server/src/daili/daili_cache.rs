use dashmap::DashMap;
use entities::entities::daili::Model as DailiModel;
use once_cell::sync::Lazy;
use tracing::error;
pub(crate) fn clear() {
    DAILI.clear();
}

pub(crate) fn cache(data: DailiModel) {
    let unique_id = if let Some(id) = data.unique_id.as_ref() {
        id.clone()
    } else {
        error!("缓存代理数据时出错：没有unique_id");
        return;
    };

    DAILI.insert(unique_id, data);
}

pub fn map<F, T>(id: &str, f: F) -> Option<T>
where
    F: FnOnce(&DailiModel) -> T,
{
    DAILI.get(id).map(|m| f(m.value()))
}

pub(crate) static DAILI: Lazy<DashMap<String, DailiModel>> = Lazy::new(DashMap::new);
