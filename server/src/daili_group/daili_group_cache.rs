use dashmap::DashMap;
use entities::entities::daili_group::Model as DailiGroupModel;
use once_cell::sync::Lazy;

pub(crate) fn clear() {
    DAILI_GROUP.clear();
}

pub(crate) fn cache(data: DailiGroupModel) {
    DAILI_GROUP.insert(data.groupid.clone(), data);
}

pub(crate) static DAILI_GROUP: Lazy<DashMap<String, DailiGroupModel>> = Lazy::new(DashMap::new);
