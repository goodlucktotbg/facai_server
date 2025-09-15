use dashmap::DashMap;
use entities::entities::fish::Model as FishModel;
use once_cell::sync::Lazy;

pub(crate) fn clear() {
    FISHES.clear();
}

#[allow(unused)]
pub(crate) fn exists(address: &str) -> bool {
    FISHES.contains_key(address)
}

pub(crate) fn find(address: &str, chain_id: &str) -> Option<i32> {
    for v in FISHES.iter() {
        if &v.fish_address == address && &v.chainid == chain_id {
            return Some(v.id);
        }
    }

    None
}

pub(crate) fn map<F, T>(address: &str, f: F) -> Option<T>
where
    F: FnOnce(&FishModel) -> T,
{
    FISHES.get(address).map(|m| f(m.value()))
}

pub(crate) fn cache(data: FishModel) {
    FISHES.insert(data.fish_address.clone(), data);
}

pub(crate) static FISHES: Lazy<DashMap<String, FishModel>> = Lazy::new(DashMap::new);
