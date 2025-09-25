use std::str::FromStr;

use anychain_core::{
    Transaction,
    crypto::{self, keccak256},
    hex,
};
use anychain_tron::{
    TronAddress, TronTransaction, TronTransactionParameters,
    abi::{Param, contract_function_call},
    trx::{build_trigger_contract, timestamp_millis},
};
use libsecp256k1::{Message, PublicKey, SecretKey, sign};
use primitive_types::U256;
use reqwest::{Client, RequestBuilder};
use serde_json::json;
use crate::tron::block::BlockBrief;
use crate::{
    options::options_cache, tron::broadcast_transaction_resp::BroadcastTransactionResp,
    utils::common::sha256d,
};

pub struct TronPublicKeyBundle {
    pub hex: String,
    pub base58: String,
}

impl TronPublicKeyBundle {
    pub fn new(base58: String, hex: String) -> Self {
        TronPublicKeyBundle { hex, base58 }
    }
}

pub fn trx_with_decimal(trx: i64) -> f64 {
    trx as f64 / 1_000_000.
}

pub fn usdt_with_decimal(usdt: i128) -> f64 {
    usdt as f64 / 1_000_000.
}

pub fn make_transaction_details_url(full_host: &str, tx_id: &str) -> String {
    let base_url = if full_host.contains("shasta") {
        "https://shasta.tronscan.org/#/transaction"
    } else {
        "https://tronscan.org/#/transaction"
    };
    format!("{base_url}/{tx_id}")
}

pub fn prepare_tron_grid_request_with_key(builder: RequestBuilder, key: &str) -> RequestBuilder {
    builder.header("TRON-PRO-API-KEY", key)
}

pub async fn send_transaction(
    client: &Client,
    signed_transaction: String,
) -> anyhow::Result<BroadcastTransactionResp> {
    let full_host = &config_helper::CONFIG.tron.full_host;
    let key = options_cache::random_one_tron_grid_key();
    send_transaction_with_key(client, full_host, signed_transaction, key.as_ref()).await
}

pub fn make_send_transaction_url(full_host: &str) -> String {
    let url = format!("{full_host}/wallet/broadcasthex");
    url
}

pub async fn send_transaction_with_key(
    client: &Client,
    full_host: &str,
    signed_transaction: String,
    api_key: Option<&String>,
) -> anyhow::Result<BroadcastTransactionResp> {
    // let full_host = &config_helper::CONFIG.tron.full_host;
    let url = make_send_transaction_url(full_host);
    let body = json!({
        "transaction": signed_transaction,
    });
    let mut client_builder = client.post(&url).json(&body);
    if let Some(key) = api_key {
        client_builder = prepare_tron_grid_request_with_key(client_builder, key);
    }
    let ret = client_builder
        .send()
        .await?
        .json::<BroadcastTransactionResp>()
        .await?;
    Ok(ret)
}

pub fn private_key_2_public_key(private_key: &str) -> anyhow::Result<TronPublicKeyBundle> {
    let priv_key_bytes = hex::decode(private_key)?;
    // let data: [u8; SECRET_KEY_SIZE] = match priv_key_bytes.try_into() {
    //     Ok(data) => data,
    //     Err(_) => return Err(anyhow::anyhow!("私钥长度不正确")),
    // };
    let secret_key = SecretKey::parse_slice(&priv_key_bytes)?;
    let public_key = PublicKey::from_secret_key(&secret_key);
    // 2. 获取未压缩公钥 (65字节，以0x04开头)
    let pubkey_uncompressed = public_key.serialize();

    // 3. keccak256 哈希 (去掉 0x04 前缀)
    let hash = keccak256(&pubkey_uncompressed[1..]);

    // 4. 取后20字节，加上 0x41 前缀
    let mut tron_addr_bytes = vec![0x41];
    tron_addr_bytes.extend_from_slice(&hash[12..]);

    // 5. Base58Check 编码 (sha256d)
    let check = &sha256d(&tron_addr_bytes)[0..4];
    let mut addr_with_check = tron_addr_bytes.clone();
    addr_with_check.extend_from_slice(check);
    let base58_addr = bs58::encode(addr_with_check).into_string();
    let hex_addr = hex::encode(&tron_addr_bytes);

    Ok(TronPublicKeyBundle::new(base58_addr, hex_addr))
}

pub fn build_contract_transaction(
    owner: &str,
    contract: &str,
    data: Vec<u8>,
    fee_limit: i64,
    block_brief: &BlockBrief,
    private_key: &str,
) -> anyhow::Result<String> {
    let contract = build_trigger_contract(owner, contract, data)?;
    let mut tx_params = TronTransactionParameters::default();
    tx_params.set_timestamp(timestamp_millis());
    tx_params.set_ref_block(block_brief.number as i64, &block_brief.block_id);
    tx_params.set_contract(contract);
    tx_params.set_fee_limit(fee_limit);
    // tx_params.set_expiration(block_brief.timestamp as i64 + rand::random_range(120_000..240_000));

    let mut transaction = TronTransaction::new(&tx_params)?;
    let signed_tx_bytes = sign_transaction(&mut transaction, private_key)?;
    Ok(hex::encode(&signed_tx_bytes))
}

pub fn build_mint_test_usdt(
    owner: &str,
    to: &str,
    amount: u64,
    block_brief: &BlockBrief,
    private_key: &str,
) -> anyhow::Result<String> {
    let to = TronAddress::from_str(to)?;
    let amount_256 = U256::from(amount);
    let params = vec![Param::from(&to), Param::from(amount_256)];
    let data = contract_function_call("mint", &params);
    let contract_address = &config_helper::CONFIG.tron.usdt_contract;

    let tx = build_contract_transaction(
        owner,
        contract_address,
        data,
        50_000_000,
        block_brief,
        private_key,
    )?;

    Ok(tx)
}

// pub fn build_transfer_trc20_transaction() -> anyhow::Result<String> {

// }

pub fn sign_transaction(
    transaction: &mut TronTransaction,
    private_key: &str,
) -> anyhow::Result<Vec<u8>> {
    let secret_key = SecretKey::parse_slice(&hex::decode(private_key)?)?;
    let tx_bytes = transaction.to_bytes()?;
    let tx_hash = crypto::sha256(&tx_bytes);

    let message = Message::parse_slice(&tx_hash)?;
    let (signature, recid) = sign(&message, &secret_key);
    let signed_tx_bytes = transaction.sign(signature.serialize().to_vec(), recid.serialize())?;

    Ok(signed_tx_bytes)
}

pub fn is_valid_trc20_address(address: &str) -> bool {
    anychain_tron::address::TronAddress::from_str(address).is_ok()
}