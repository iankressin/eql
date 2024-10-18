use super::{
    block::BlockRange,
    entity_id::{parse_block_number_or_tag, EntityIdError},
};
use crate::interpreter::frontend::parser::{ParserError, Rule};
use alloy::{
    eips::BlockNumberOrTag,
    hex::FromHexError,
    primitives::{Address, AddressError, B256},
    rpc::types::Filter,
};
use eql_macros::EnumVariants;
use pest::iterators::{Pair, Pairs};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum LogEntityError {
    InvalidField(String),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EntityFilterError {
    #[error("Invalid block number")]
    InvalidBlockNumber,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Logs {
    filter: Vec<LogFilter>,
    fields: Vec<LogField>,
}

impl Logs {
    pub fn new(filter: Vec<LogFilter>, fields: Vec<LogField>) -> Self {
        Self { filter, fields }
    }

    pub fn filter(&self) -> &Vec<LogFilter> {
        &self.filter
    }

    pub fn fields(&self) -> &Vec<LogField> {
        &self.fields
    }

    pub fn build_bloom_filter(&self) -> Filter {
        LogFilter::build_filter(&self.filter)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LogsError {
    #[error("Invalid log filter {0}")]
    InvalidLogFilter(String),

    #[error(transparent)]
    FromHexError(#[from] FromHexError),

    #[error(transparent)]
    ParserError(#[from] ParserError),

    #[error(transparent)]
    EntityIdError(#[from] EntityIdError),

    #[error(transparent)]
    AddressError(#[from] AddressError),

    #[error(transparent)]
    LogFieldError(#[from] LogFieldError),
}

impl TryFrom<Pairs<'_, Rule>> for Logs {
    type Error = LogsError;

    fn try_from(pairs: Pairs<'_, Rule>) -> Result<Self, Self::Error> {
        let mut filter: Vec<LogFilter> = Vec::new();
        let mut fields: Vec<LogField> = Vec::new();

        for pair in pairs {
            match pair.as_rule() {
                Rule::log_filter => {
                    let mut inner_pairs = pair.into_inner();
                    let pair = inner_pairs.next();

                    if let Some(pair) = pair {
                        filter.push(LogFilter::try_from(pair)?);
                    } else {
                        return Err(LogsError::InvalidLogFilter(
                            inner_pairs.as_str().to_string(),
                        ));
                    }
                }
                Rule::log_fields => {
                    let inner_pairs = pair.into_inner();

                    if let Some(pair) = inner_pairs.peek() {
                        if pair.as_rule() == Rule::wildcard {
                            fields = LogField::all_variants().to_vec();
                            continue;
                        }
                    }

                    fields = inner_pairs
                        .map(|pair| LogField::try_from(pair.as_str()))
                        .collect::<Result<Vec<LogField>, LogFieldError>>()?;
                }
                _ => {
                    return Err(LogsError::InvalidLogFilter(pair.as_str().to_string()));
                }
            }
        }

        Ok(Logs { filter, fields })
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum LogFilter {
    BlockRange(BlockRange),
    BlockHash(B256),
    EmitterAddress(Address),
    EventSignature(String),
    Topic0(B256),
    Topic1(B256),
    Topic2(B256),
    Topic3(B256),
}

impl TryFrom<Pair<'_, Rule>> for LogFilter {
    type Error = LogsError;

    fn try_from(pair: Pair<'_, Rule>) -> Result<Self, Self::Error> {
        match pair.as_rule() {
            Rule::address_filter => {
                let tochecksum = pair.as_str().trim_start_matches("address ").trim();
                let address = Address::parse_checksummed(tochecksum, None)?;
                Ok(LogFilter::EmitterAddress(address))
            }
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
                Ok(LogFilter::BlockRange(BlockRange::new(start, end)))
            }
            Rule::blockhash_filter => {
                let hash = pair
                    .as_str()
                    .trim_start_matches("blockhash ")
                    .trim_start_matches("block_hash ")
                    .trim();
                let hash = hash.parse::<B256>()?;
                Ok(LogFilter::BlockHash(hash))
            }
            Rule::event_signature_filter => {
                let signature = pair.as_str().trim_start_matches("event_signature ").trim();
                Ok(LogFilter::EventSignature(signature.to_string()))
            }
            Rule::topic0_filter => {
                let topic = pair.as_str().trim_start_matches("topic0 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(LogFilter::Topic0(topic))
            }
            Rule::topic1_filter => {
                let topic = pair.as_str().trim_start_matches("topic1 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(LogFilter::Topic1(topic))
            }
            Rule::topic2_filter => {
                let topic = pair.as_str().trim_start_matches("topic2 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(LogFilter::Topic2(topic))
            }
            Rule::topic3_filter => {
                let topic = pair.as_str().trim_start_matches("topic3 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(LogFilter::Topic3(topic))
            }
            _ => Err(LogsError::InvalidLogFilter(pair.as_str().to_string())),
        }
    }
}

impl LogFilter {
    // TODO: remove this method
    pub fn to_block_range(
        &self,
    ) -> Result<(BlockNumberOrTag, Option<BlockNumberOrTag>), EntityFilterError> {
        match self {
            LogFilter::BlockRange(block_id) => Ok(block_id.range()),
            _ => Err(EntityFilterError::InvalidBlockNumber),
        }
    }

    fn to_filter(&self, filter: Filter) -> Filter {
        match self {
            LogFilter::BlockRange(range) => {
                filter
                    .from_block(range.start())
                    // If end is None, range is actually one block. unwrap_or will reuse start as range
                    .to_block(range.end().unwrap_or(range.start()))
            }
            LogFilter::BlockHash(hash) => filter.at_block_hash(*hash),
            LogFilter::EmitterAddress(address) => filter.address(*address),
            LogFilter::EventSignature(signature) => filter.event(signature),
            LogFilter::Topic0(topic_hash) => filter.event_signature(*topic_hash),
            LogFilter::Topic1(topic_hash) => filter.topic1(*topic_hash),
            LogFilter::Topic2(topic_hash) => filter.topic2(*topic_hash),
            LogFilter::Topic3(topic_hash) => filter.topic3(*topic_hash),
        }
    }

    pub fn build_filter(entity_filters: &[LogFilter]) -> Filter {
        entity_filters
            .iter()
            .fold(Filter::new(), |filter, entity_filter| {
                entity_filter.to_filter(filter)
            })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, EnumVariants)]
pub enum LogField {
    Address,
    Topic0,
    Topic1,
    Topic2,
    Topic3,
    Data,
    BlockHash,
    BlockNumber,
    BlockTimestamp,
    TransactionHash,
    TransactionIndex,
    LogIndex,
    Removed,
}

impl std::fmt::Display for LogField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogField::Address => write!(f, "address"),
            LogField::Topic0 => write!(f, "topic0"),
            LogField::Topic1 => write!(f, "topic1"),
            LogField::Topic2 => write!(f, "topic2"),
            LogField::Topic3 => write!(f, "topic3"),
            LogField::Data => write!(f, "data"),
            LogField::BlockHash => write!(f, "block_hash"),
            LogField::BlockNumber => write!(f, "block_number"),
            LogField::BlockTimestamp => write!(f, "block_timestamp"),
            LogField::TransactionHash => write!(f, "transaction_hash"),
            LogField::TransactionIndex => write!(f, "transaction_index"),
            LogField::LogIndex => write!(f, "log_index"),
            LogField::Removed => write!(f, "removed"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LogFieldError {
    #[error("Invalid log field: {0}")]
    InvalidLogField(String),
}

impl TryFrom<&str> for LogField {
    type Error = LogFieldError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "address" => Ok(LogField::Address),
            "topic0" => Ok(LogField::Topic0),
            "topic1" => Ok(LogField::Topic1),
            "topic2" => Ok(LogField::Topic2),
            "topic3" => Ok(LogField::Topic3),
            "data" => Ok(LogField::Data),
            "block_hash" => Ok(LogField::BlockHash),
            "block_number" => Ok(LogField::BlockNumber),
            "block_timestamp" => Ok(LogField::BlockTimestamp),
            "transaction_hash" => Ok(LogField::TransactionHash),
            "transaction_index" => Ok(LogField::TransactionIndex),
            "log_index" => Ok(LogField::LogIndex),
            "removed" => Ok(LogField::Removed),
            invalid_field => Err(LogFieldError::InvalidLogField(invalid_field.to_string())),
        }
    }
}
