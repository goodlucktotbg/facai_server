use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct BlockBrief {
    pub block_id: String,
    pub number: u64,
    pub timestamp: u64,
}

/// 当前区块
pub static CURRENT_BLOCK: Lazy<RwLock<Option<BlockBrief>>> = Lazy::new(|| RwLock::new(None));

pub async fn update_block_brief(block: &Block) {
    let block_id = block.block_id.clone();
    let number = block.block_header.raw_data.number;
    let timestamp = block.block_header.raw_data.timestamp;
    let mut write = CURRENT_BLOCK.write().await;
    write.replace(BlockBrief { block_id, number, timestamp });
}

pub async fn get_block_brief() -> Option<BlockBrief> {
    CURRENT_BLOCK.read().await.clone()
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum ContractType {
    AccountCreateContract,
    TransferContract,
    TransferAssetContract,
    VoteAssetContract,
    VoteWitnessContract,
    WitnessCreateContract,
    AssetIssueContract,
    WitnessUpdateContract,
    ParticipateAssetIssueContract,
    AccountUpdateContract,
    FreezeBalanceContract,
    UnfreezeBalanceContract,
    CancelAllUnfreezeV2Contract,
    WithdrawBalanceContract,
    UnfreezeAssetContract,
    UpdateAssetContract,
    ProposalCreateContract,
    ProposalApproveContract,
    ProposalDeleteContract,
    SetAccountIdContract,
    CustomContract,
    CreateSmartContract,
    TriggerSmartContract,
    GetContract,
    UpdateSettingContract,
    ExchangeCreateContract,
    ExchangeInjectContract,
    ExchangeWithdrawContract,
    ExchangeTransactionContract,
    UpdateEnergyLimitContract,
    AccountPermissionUpdateContract,
    ClearABIContract,
    UpdateBrokerageContract,
    ShieldedTransferContract,
    MarketSellAssetContract,
    MarketCancelOrderContract,
    FreezeBalanceV2Contract,
    UnfreezeBalanceV2Contract,
    WithdrawExpireUnfreezeContract,
    DelegateResourceContract,
    UnDelegateResourceContract,
    UNRECOGNIZED,
}

#[derive(Serialize, Deserialize)]
pub struct BlockRawData {
    pub number: u64,
    #[serde(rename = "txTrieRoot")]
    pub tx_trie_root: String,
    pub witness_id: Option<u64>,
    pub witness_address: String,
    #[serde(rename = "parentHash")]
    pub parent_hash: String,
    #[serde(rename = "accountStateRoot")]
    pub account_state_root: Option<String>,
    pub version: u32,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize)]
pub struct BlockHeader {
    pub raw_data: BlockRawData,
    pub witness_signature: String,
}

#[derive(Serialize, Deserialize)]
pub struct Block {
    #[serde(rename = "blockID")]
    pub block_id: String,
    pub block_header: BlockHeader,
    pub transactions: Option<Vec<Transaction>>,
}

#[derive(Serialize, Deserialize)]
pub struct TransactionRet {
    #[serde(rename = "contractRet")]
    pub contract_ret: ContractRet,
}

#[derive(Serialize, Deserialize)]
pub struct Transaction {
    pub ret: Vec<TransactionRet>,
    #[serde(rename = "txID")]
    pub tx_id: String,
    pub raw_data: TransactionRawData,
    pub signature: Vec<String>,
    pub raw_data_hex: String,
}

#[derive(Serialize, Deserialize)]
pub enum ContractRet {
    SUCCESS,
    #[serde(rename = "OUT_OF_ENERGY")]
    OutOfEnergy,
    FAIL,
    FAILED,
    REVERT,
}

#[derive(Serialize, Deserialize)]
pub struct TransactionRawData {
    pub contract: Option<Vec<TransactionContract>>,
    pub ref_block_bytes: String,
    pub ref_block_hash: String,
    pub expiration: u64,
    pub timestamp: u64,
    pub fee_limit: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct TransactionContract {
    pub parameter: Option<TransactonContractParameter>,
    pub r#type: ContractType,
}

#[derive(Serialize, Deserialize)]
pub struct TransactonContractParameter {
    pub value: Option<Value>, // 暂时不序列化，使用时根据类型再反序列化
    pub type_url: String,
}

#[derive(Serialize, Deserialize)]
pub struct TransactionContractParameterValue {
    pub data: String,
    pub owner_address: String,
    pub contract_address: String,
}
