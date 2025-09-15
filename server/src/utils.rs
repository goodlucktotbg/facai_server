use chrono::Local;
use rand::seq::SliceRandom;

pub mod common;
pub mod tron;

pub fn random_one<T>(slice: &mut [T]) -> Option<&T> {
    let mut rng = rand::rng();
    let (r, _) = slice.partial_shuffle(&mut rng, 1);
    r.first()
}

pub fn now_date_time_str() -> String {
    let now = Local::now();
    now.format("%Y-%m-%d %H:%M:%S %:z").to_string()
}
