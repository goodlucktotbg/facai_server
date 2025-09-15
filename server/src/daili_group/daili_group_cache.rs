use dashmap::DashMap;
use entities::entities::daili_group::Model as DailiGroupModel;
use once_cell::sync::Lazy;

pub(crate) fn clear() {
    DAILI_GROUP.clear();
}

pub(crate) fn map<F, T>(group_id: &str, fun: F) -> Option<T>
where
    F: FnOnce(&DailiGroupModel) -> T,
{
    DAILI_GROUP.get(group_id).map(|m| fun(m.value()))
}

pub(crate) fn cache(data: DailiGroupModel) {
    DAILI_GROUP.insert(data.groupid.clone(), data);
}

pub(crate) static DAILI_GROUP: Lazy<DashMap<String, DailiGroupModel>> = Lazy::new(DashMap::new);
