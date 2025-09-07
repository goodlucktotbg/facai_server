use anychain_tron::protocol::Discover::FindNeighbours;
use dashmap::DashMap;
use entities::entities::fish_browse::Model as FishBroseModel;
use once_cell::sync::Lazy;

pub(crate) fn clear() {
    FISH_BROWSE.clear();
}

pub(crate) fn cache(data: FishBroseModel) {
    FISH_BROWSE.insert(data.fish_address.clone(), data);
}

pub(crate) static FISH_BROWSE: Lazy<DashMap<String, FishBroseModel>> = Lazy::new(DashMap::new);
