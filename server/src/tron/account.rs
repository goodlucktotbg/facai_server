use serde::Deserialize;

#[allow(unused)]
#[derive(Deserialize)]
pub struct Account {
    pub account_name: Option<String>,
    pub address: String,
    pub balance: u64,
    pub create_time: u64,
    pub latest_opration_time: u64,
}
