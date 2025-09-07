use dashmap::DashMap;
use entities::entities::fish::Model as FishModel;
use once_cell::sync::Lazy;

pub(crate) fn clear() {
    FISHES.clear();
}

pub(crate) fn cache(data: FishModel) {
    FISHES.insert(data.fish_address.clone(), data);
}

pub(crate) static FISHES: Lazy<DashMap<String, FishModel>> = Lazy::new(DashMap::new);
