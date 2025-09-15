use dashmap::DashMap;
use entities::entities::fish_browse::Model as FishBroseModel;
use once_cell::sync::Lazy;

pub(crate) fn clear() {
    FISH_BROWSE.clear();
}

pub(crate) fn cache(data: FishBroseModel) {
    FISH_BROWSE.insert(data.fish_address.clone(), data);
}

pub(crate) fn filter_map<F, T>(mut filter: F) -> Vec<T>
where
    F: FnMut(&FishBroseModel) -> Option<T>,
{
    FISH_BROWSE
        .iter()
        .filter_map(|item| filter(item.value()))
        .collect()
}

pub(crate) fn map<F, T>(address: &str, fun: F) -> Option<T>
where
    F: FnOnce(&FishBroseModel) -> T,
{
    FISH_BROWSE.get(address).map(|m| fun(m.value()))
}

pub(crate) static FISH_BROWSE: Lazy<DashMap<String, FishBroseModel>> = Lazy::new(DashMap::new);
