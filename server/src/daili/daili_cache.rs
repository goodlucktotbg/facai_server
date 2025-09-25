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

pub fn exist_unique_id(unique_id: &str) -> bool {
    DAILI.contains_key(unique_id)
}

pub fn map_by_user_id_group_id<F, T>(tg_uid: &str, group_id: &str, fun: F) -> Option<T>
where
    F: FnOnce(&DailiModel) -> T,
{
    for item in DAILI.iter() {
        let v = item.value();
        if v.tguid != tg_uid {
            continue;
        }
        if let Some(this_group_id) = &v.groupid {
            if group_id == this_group_id {
                let ret = fun(v);
                return Some(ret);
            }
        }
    }

    None
}

pub fn map<F, T>(unique_id: &str, f: F) -> Option<T>
where
    F: FnOnce(&DailiModel) -> T,
{
    DAILI.get(unique_id).map(|m| f(m.value()))
}

pub(crate) static DAILI: Lazy<DashMap<String, DailiModel>> = Lazy::new(DashMap::new);
