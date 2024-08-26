use super::{
    chain::Chain,
    entity::Entity,
    entity_id::{BlockRange, EntityId},
};
use alloy::eips::BlockNumberOrTag;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt::Display};

#[derive(Debug, PartialEq, Eq)]
pub enum Expression {
    Get(GetExpression),
}

#[derive(Debug, PartialEq, Eq)]
pub struct GetExpression {
    pub entity: Entity,
    pub entity_id: EntityId,
    pub fields: Vec<Field>,
    pub chain: Chain,
    pub query: String,
    pub dump: Option<Dump>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Dump {
    pub name: String,
    pub format: DumpFormat,
}

impl Dump {
    pub fn new(name: String, format: DumpFormat) -> Self {
        Self { name, format }
    }

    pub fn path(&self) -> String {
        format!("{}.{}", self.name, self.format)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DumpError {
    #[error("Invalid dump: {0}")]
    InvalidDump(String),
}

impl TryFrom<&str> for Dump {
    type Error = Box<dyn Error>;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let parts: Vec<&str> = value.split('.').collect();

        if parts.len() != 2 {
            return Err(Box::new(DumpError::InvalidDump(value.to_string())));
        }

        let name = parts[0].to_string().replace(">", "").trim().to_string();
        let format = DumpFormat::try_from(parts[1])?;

        Ok(Dump::new(name, format))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum DumpFormat {
    Json,
    Csv,
    Parquet,
}

impl TryFrom<&str> for DumpFormat {
    type Error = Box<dyn Error>;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "json" => Ok(DumpFormat::Json),
            "csv" => Ok(DumpFormat::Csv),
            "parquet" => Ok(DumpFormat::Parquet),
            invalid_format => Err(Box::new(DumpError::InvalidDump(invalid_format.to_string()))),
        }
    }
}

impl Display for DumpFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DumpFormat::Json => write!(f, "json"),
            DumpFormat::Csv => write!(f, "csv"),
            DumpFormat::Parquet => write!(f, "parquet"),
        }
    }
}

impl Default for GetExpression {
    fn default() -> Self {
        Self {
            entity: Entity::Block,
            entity_id: EntityId::Block(BlockRange::new(BlockNumberOrTag::Earliest, None)),
            fields: vec![],
            chain: Chain::Ethereum,
            query: "".to_string(),
            dump: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Field {
    Account(AccountField),
    Block(BlockField),
    Transaction(TransactionField),
}

impl Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Field::Account(account_field) => write!(f, "{}", account_field),
            Field::Block(block_field) => write!(f, "{}", block_field),
            Field::Transaction(transaction_field) => write!(f, "{}", transaction_field),
        }
    }
}

#[derive(Debug)]
pub enum FieldError {
    InvalidField(String),
}

impl Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldError::InvalidField(field) => write!(f, "Invalid field: {}", field),
        }
    }
}

impl Error for FieldError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl TryFrom<&Field> for AccountField {
    type Error = Box<dyn Error>;

    fn try_from(field: &Field) -> Result<Self, Self::Error> {
        match field {
            Field::Account(account_field) => Ok(*account_field),
            invalid_field => Err(Box::new(FieldError::InvalidField(format!(
                "Invalid field {:?} for entity account",
                invalid_field
            )))),
        }
    }
}

impl TryFrom<&str> for AccountField {
    type Error = Box<dyn Error>;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "address" => Ok(AccountField::Address),
            "nonce" => Ok(AccountField::Nonce),
            "balance" => Ok(AccountField::Balance),
            "code" => Ok(AccountField::Code),
            invalid_field => Err(Box::new(FieldError::InvalidField(
                invalid_field.to_string(),
            ))),
        }
    }
}

impl TryFrom<&Field> for BlockField {
    type Error = Box<dyn Error>;

    fn try_from(field: &Field) -> Result<Self, Self::Error> {
        match field {
            Field::Block(block_field) => Ok(*block_field),
            invalid_field => Err(Box::new(FieldError::InvalidField(format!(
                "Invalid field {:?} for entity block",
                invalid_field
            )))),
        }
    }
}

impl TryFrom<&str> for BlockField {
    type Error = Box<dyn Error>;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "number" => Ok(BlockField::Number),
            "timestamp" => Ok(BlockField::Timestamp),
            "size" => Ok(BlockField::Size),
            "hash" => Ok(BlockField::Hash),
            "parent_hash" => Ok(BlockField::ParentHash),
            "state_root" => Ok(BlockField::StateRoot),
            "transactions_root" => Ok(BlockField::TransactionsRoot),
            "receipts_root" => Ok(BlockField::ReceiptsRoot),
            "logs_bloom" => Ok(BlockField::LogsBloom),
            "extra_data" => Ok(BlockField::ExtraData),
            "mix_hash" => Ok(BlockField::MixHash),
            "total_difficulty" => Ok(BlockField::TotalDifficulty),
            "base_fee_per_gas" => Ok(BlockField::BaseFeePerGas),
            "withdrawals_root" => Ok(BlockField::WithdrawalsRoot),
            "blob_gas_used" => Ok(BlockField::BlobGasUsed),
            "excess_blob_gas" => Ok(BlockField::ExcessBlobGas),
            "parent_beacon_block_root" => Ok(BlockField::ParentBeaconBlockRoot),
            invalid_field => Err(Box::new(FieldError::InvalidField(
                invalid_field.to_string(),
            ))),
        }
    }
}

impl TryFrom<&Field> for TransactionField {
    type Error = Box<dyn Error>;

    fn try_from(field: &Field) -> Result<Self, Self::Error> {
        match field {
            Field::Transaction(transaction_field) => Ok(*transaction_field),
            invalid_field => Err(Box::new(FieldError::InvalidField(format!(
                "Invalid field {:?} for entity transaction",
                invalid_field
            )))),
        }
    }
}

impl TryFrom<&str> for TransactionField {
    type Error = Box<dyn Error>;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "transaction_type" => Ok(TransactionField::TransactionType),
            "hash" => Ok(TransactionField::Hash),
            "from" => Ok(TransactionField::From),
            "to" => Ok(TransactionField::To),
            "data" => Ok(TransactionField::Data),
            "value" => Ok(TransactionField::Value),
            "gas_price" => Ok(TransactionField::GasPrice),
            "gas" => Ok(TransactionField::Gas),
            "status" => Ok(TransactionField::Status),
            "chain_id" => Ok(TransactionField::ChainId),
            "v" => Ok(TransactionField::V),
            "r" => Ok(TransactionField::R),
            "s" => Ok(TransactionField::S),
            "max_fee_per_blob_gas" => Ok(TransactionField::MaxFeePerBlobGas),
            "max_fee_per_gas" => Ok(TransactionField::MaxFeePerGas),
            "max_priority_fee_per_gas" => Ok(TransactionField::MaxPriorityFeePerGas),
            "y_parity" => Ok(TransactionField::YParity),
            invalid_field => Err(Box::new(FieldError::InvalidField(
                invalid_field.to_string(),
            ))),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum AccountField {
    Address,
    Nonce,
    Balance,
    Code,
}

impl Display for AccountField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountField::Address => write!(f, "address"),
            AccountField::Nonce => write!(f, "nonce"),
            AccountField::Balance => write!(f, "balance"),
            AccountField::Code => write!(f, "code"),
        }
    }
}

// TODO: should include nonce, transactions and withdrawals
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum BlockField {
    Number,
    Timestamp,
    Size,
    Hash,
    ParentHash,
    StateRoot,
    TransactionsRoot,
    ReceiptsRoot,
    LogsBloom,
    ExtraData,
    MixHash,
    TotalDifficulty,
    BaseFeePerGas,
    WithdrawalsRoot,
    BlobGasUsed,
    ExcessBlobGas,
    ParentBeaconBlockRoot,
}

impl Display for BlockField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockField::Number => write!(f, "number"),
            BlockField::Timestamp => write!(f, "timestamp"),
            BlockField::Size => write!(f, "size"),
            BlockField::Hash => write!(f, "hash"),
            BlockField::ParentHash => write!(f, "parent_hash"),
            BlockField::StateRoot => write!(f, "state_root"),
            BlockField::TransactionsRoot => write!(f, "transactions_root"),
            BlockField::ReceiptsRoot => write!(f, "receipts_root"),
            BlockField::LogsBloom => write!(f, "logs_bloom"),
            BlockField::ExtraData => write!(f, "extra_data"),
            BlockField::MixHash => write!(f, "mix_hash"),
            BlockField::TotalDifficulty => write!(f, "total_difficulty"),
            BlockField::BaseFeePerGas => write!(f, "base_fee_per_gas"),
            BlockField::WithdrawalsRoot => write!(f, "withdrawals_root"),
            BlockField::BlobGasUsed => write!(f, "blob_gas_used"),
            BlockField::ExcessBlobGas => write!(f, "excess_blob_gas"),
            BlockField::ParentBeaconBlockRoot => write!(f, "parent_beacon_block_root"),
        }
    }
}

// TODO: implement blob_versioned_hashes and access_list
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum TransactionField {
    TransactionType,
    Hash,
    From,
    To,
    Data,
    Value,
    GasPrice,
    Gas,
    Status,
    ChainId,
    V,
    R,
    S,
    MaxFeePerBlobGas,
    MaxFeePerGas,
    MaxPriorityFeePerGas,
    YParity,
}

impl std::fmt::Display for TransactionField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionField::TransactionType => write!(f, "type"),
            TransactionField::Hash => write!(f, "hash"),
            TransactionField::From => write!(f, "from"),
            TransactionField::To => write!(f, "to"),
            TransactionField::Data => write!(f, "data"),
            TransactionField::Value => write!(f, "value"),
            TransactionField::GasPrice => write!(f, "gas_price"),
            TransactionField::Gas => write!(f, "gas"),
            TransactionField::Status => write!(f, "status"),
            TransactionField::ChainId => write!(f, "chain_id"),
            TransactionField::V => write!(f, "v"),
            TransactionField::R => write!(f, "r"),
            TransactionField::S => write!(f, "s"),
            TransactionField::MaxFeePerBlobGas => write!(f, "max_fee_per_blob_gas"),
            TransactionField::MaxFeePerGas => write!(f, "max_fee_per_gas"),
            TransactionField::MaxPriorityFeePerGas => write!(f, "max_priority_fee_per_gas"),
            TransactionField::YParity => write!(f, "y_parity"),
        }
    }
}
