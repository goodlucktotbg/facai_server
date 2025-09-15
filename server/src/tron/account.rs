use serde::Deserialize;

#[allow(unused)]
#[derive(Deserialize)]
pub struct Account {
    pub account_name: String,
    pub address: String,
    pub balance: u64,
}
