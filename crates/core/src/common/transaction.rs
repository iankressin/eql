use super::{
    block::BlockRange, ens::NameOrAddress, entity_id::parse_block_number_or_tag,
    entity_id::EntityIdError,
};
use crate::interpreter::frontend::parser::{ParserError, Rule};
use alloy::{
    hex::FromHexError,
    primitives::{bytes::Bytes, AddressError, B256, U256},
};
use eql_macros::EnumVariants;
use pest::iterators::{Pair, Pairs};
use serde::{Deserialize, Serialize};
use std::{error::Error, str::FromStr};

#[derive(Debug, PartialEq, Eq)]
pub struct Transaction {
    ids: Option<Vec<B256>>,
    filter: Option<Vec<TransactionFilter>>,
    fields: Vec<TransactionField>,
}

impl Transaction {
    pub fn new(
        ids: Option<Vec<B256>>,
        filter: Option<Vec<TransactionFilter>>,
        fields: Vec<TransactionField>,
    ) -> Self {
        Self {
            ids,
            filter,
            fields,
        }
    }

    pub fn ids(&self) -> Option<&Vec<B256>> {
        self.ids.as_ref()
    }

    pub fn fields(&self) -> &Vec<TransactionField> {
        &self.fields
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TransactionError {
    #[error("Unexpected token {0} for transaction")]
    UnexpectedToken(String),

    #[error(transparent)]
    EntityIdError(#[from] EntityIdError),

    #[error(transparent)]
    FromHexError(#[from] FromHexError),

    #[error(transparent)]
    AddressError(#[from] AddressError),

    #[error(transparent)]
    TransactionFieldError(#[from] TransactionFieldError),

    #[error(transparent)]
    TransactionFilterError(#[from] TransactionFilterError),
}

impl TryFrom<Pairs<'_, Rule>> for Transaction {
    type Error = TransactionError;

    fn try_from(pairs: Pairs<'_, Rule>) -> Result<Self, Self::Error> {
        let mut ids: Option<Vec<B256>> = None;
        let mut filter: Option<Vec<TransactionFilter>> = None;
        let mut fields: Vec<TransactionField> = vec![];

        for pair in pairs {
            match pair.as_rule() {
                Rule::tx_id => {
                    if let Some(ids) = ids.as_mut() {
                        ids.push(B256::from_str(pair.as_str())?);
                    } else {
                        ids = Some(vec![B256::from_str(pair.as_str())?]);
                    }
                }
                Rule::tx_filter => {
                    filter = Some(
                        pair.into_inner()
                            .map(|pair| TransactionFilter::try_from(pair))
                            .collect::<Result<Vec<TransactionFilter>, TransactionFilterError>>()?,
                    );
                }
                Rule::tx_fields => {
                    let inner_pairs = pair.into_inner();

                    if let Some(pair) = inner_pairs.peek() {
                        if pair.as_rule() == Rule::wildcard {
                            fields = TransactionField::all_variants().to_vec();
                            continue;
                        }
                    }
                    fields = inner_pairs
                        .map(|pair| TransactionField::try_from(pair.as_str()))
                        .collect::<Result<Vec<TransactionField>, TransactionFieldError>>()?;
                }
                _ => {
                    return Err(TransactionError::UnexpectedToken(pair.as_str().to_string()));
                }
            }
        }

        Ok(Transaction {
            ids,
            filter,
            fields,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, EnumVariants)]
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

#[derive(thiserror::Error, Debug)]
pub enum TransactionFieldError {
    #[error("Invalid transaction field: {0}")]
    InvalidTransactionField(String),
}

// TODO: this can possibly be removed as we're using TryFrom<Pair<'_, Rule>> for TransactionField
impl TryFrom<&str> for TransactionField {
    type Error = TransactionFieldError;

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
            invalid_field => Err(TransactionFieldError::InvalidTransactionField(
                invalid_field.to_string(),
            )),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TransactionFilter {
    TransactionType(u8),
    Hash(B256),
    From(NameOrAddress),
    To(NameOrAddress),
    Data(Bytes),
    Value(U256),
    GasPrice(u128),
    Gas(u128),
    ChainId(u64),
    BlockRange(BlockRange),
    Status(bool),
    V(U256),
    R(U256),
    S(U256),
    MaxFeePerBlobGas(u128),
    MaxFeePerGas(u128),
    MaxPriorityFeePerGas(u128),
    YParity(bool),
}

#[derive(thiserror::Error, Debug)]
pub enum TransactionFilterError {
    #[error("Invalid transaction filter property: {0}")]
    InvalidTransactionFilterProperty(String),

    #[error(transparent)]
    EntityIdError(#[from] EntityIdError),

    #[error(transparent)]
    FromHexError(#[from] FromHexError),
}

impl TryFrom<Pair<'_, Rule>> for TransactionFilter {
    type Error = TransactionFilterError;

    fn try_from(pair: Pair<'_, Rule>) -> Result<Self, Self::Error> {
        match pair.as_rule() {
            // TODO: this implementation is a copy of BlockFilter::try_from, we should refactor this.
            Rule::blockrange_filter => {
                let range = pair.as_str().trim_start_matches("block ").trim();
                let (start, end) = match range.split_once(":") {
                    //if ":" is present, we have an start and an end.
                    Some((start, end)) => (
                        parse_block_number_or_tag(start)?,
                        Some(parse_block_number_or_tag(end)?),
                    ),
                    //else we only have start.
                    None => (parse_block_number_or_tag(range)?, None),
                };
                Ok(TransactionFilter::BlockRange(BlockRange::new(start, end)))
            }
            Rule::from_filter => Ok(TransactionFilter::From(NameOrAddress::from_str(
                pair.as_str(),
            )?)),
            Rule::to_filter => Ok(TransactionFilter::To(NameOrAddress::from_str(
                pair.as_str(),
            )?)),
            Rule::data_filter => Ok(TransactionFilter::Data(Bytes::from(
                pair.as_str().to_string(),
            ))),
            _ => {
                return Err(TransactionFilterError::InvalidTransactionFilterProperty(
                    pair.as_str().to_string(),
                ));
            }
        }
    }
}
