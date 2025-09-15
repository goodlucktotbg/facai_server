use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct BroadcastTransactionResp {
    pub result: bool,
    #[serde(rename = "txid")]
    pub tx_id: String,
    pub code: Option<String>,
    pub message: Option<String>,
    pub transaction: Option<Value>,
}

#[allow(unused)]
impl BroadcastTransactionResp {
    pub fn send_success(&self) -> bool {
        self.result
    }

    pub fn success(&self) -> bool {
        self.result && self.code.as_ref().map(|c| c == "SUCCESS").unwrap_or(false)
    }
}
