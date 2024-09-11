use alloy::{
    eips::BlockNumberOrTag,
    rpc::types::Filter,
    primitives::{Address, B256},
};
use std::{error::Error, fmt::{self, Display, Formatter}};
use pest::iterators::Pair;
use crate::interpreter::frontend::parser::{ParserError, Rule};
use super::entity_id::parse_block_number_or_tag;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EntityFilterError {
    #[error("Invalid block number")]
    InvalidBlockNumber,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EntityFilter {
    LogBlockRange(BlockRange),
    LogBlockHash(B256),
    LogEmitterAddress(Address),
    LogEventSignature(String),
    LogTopic0(B256),
    LogTopic1(B256),    
    LogTopic2(B256),
    LogTopic3(B256),
}

impl<'a> TryFrom<Pair<'a, Rule>> for EntityFilter {
    type Error = Box<dyn Error>;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        match pair.as_rule() {
            Rule::address_filter => {
                let tochecksum = pair.as_str().trim_start_matches("address ").trim();
                let address = Address::parse_checksummed(tochecksum, None)
                    .map_err(|e| format!("{}: {}", e, tochecksum))?;
                Ok(EntityFilter::LogEmitterAddress(address))
            },
            Rule::blockrange_filter => {
                let range = pair.as_str().trim_start_matches("block ").trim();
                let (start, end) = match range.split_once(":") {
                    //if ":" is present, we have an start and an end.
                    Some((start, end)) => (
                        parse_block_number_or_tag(start)?,
                        Some(parse_block_number_or_tag(end)?),
                    ),
                    //else we only have start.
                    None => (
                        parse_block_number_or_tag(range)?,
                        None,
                    ),
                };
                Ok(EntityFilter::LogBlockRange(BlockRange { start, end }))
            }
            Rule::blockhash_filter => {
                let hash = pair.as_str().trim_start_matches("blockhash ").trim_start_matches("block_hash ").trim();
                let hash = hash.parse::<B256>()?;
                Ok(EntityFilter::LogBlockHash(hash))
            }
            Rule::event_signature_filter => {
                let signature = pair.as_str().trim_start_matches("event_signature ").trim();
                Ok(EntityFilter::LogEventSignature(signature.to_string()))
            }
            Rule::topic0_filter => {
                let topic = pair.as_str().trim_start_matches("topic0 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(EntityFilter::LogTopic0(topic))
            }
            Rule::topic1_filter => {
                let topic = pair.as_str().trim_start_matches("topic1 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(EntityFilter::LogTopic1(topic))
            }
            Rule::topic2_filter => {
                let topic = pair.as_str().trim_start_matches("topic2 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(EntityFilter::LogTopic2(topic))
            }
            Rule::topic3_filter => {
                let topic = pair.as_str().trim_start_matches("topic3 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(EntityFilter::LogTopic3(topic))
            }
            _ => Err(Box::new(ParserError::UnexpectedToken(pair.as_str().to_string()))),
        }
    }
}

impl EntityFilter {
    pub fn to_block_range(
        &self,
    ) -> Result<(BlockNumberOrTag, Option<BlockNumberOrTag>), EntityFilterError> {
        match self {
            EntityFilter::LogBlockRange(block_id) => Ok(block_id.range()),
            _ => Err(EntityFilterError::InvalidBlockNumber),
        }
    }

    fn to_filter(&self, filter: Filter) -> Filter {
        match self {
            EntityFilter::LogBlockRange(range) => {
                filter.from_block(range.start)
                    // If end is None, range is actually one block. unwrap_or will reuse start as range  
                    .to_block(range.end.unwrap_or(range.start))
            },
            EntityFilter::LogBlockHash(hash) => filter.at_block_hash(*hash),
            EntityFilter::LogEmitterAddress(address) => filter.address(*address),
            EntityFilter::LogEventSignature(signature) => filter.event(signature),
            EntityFilter::LogTopic0(topic_hash) => filter.event_signature(*topic_hash),
            EntityFilter::LogTopic1(topic_hash) => filter.topic1(*topic_hash),
            EntityFilter::LogTopic2(topic_hash) => filter.topic2(*topic_hash),
            EntityFilter::LogTopic3(topic_hash) => filter.topic3(*topic_hash),
        }

    }

    pub fn build_filter(entity_filters: &[EntityFilter]) -> Filter {
        entity_filters
            .iter()
            .fold(Filter::new(), |filter, entity_filter| {
                entity_filter.to_filter(filter)
            })
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BlockRange {
    start: BlockNumberOrTag,
    end: Option<BlockNumberOrTag>,
}

impl BlockRange {
    pub fn new(start: BlockNumberOrTag, end: Option<BlockNumberOrTag>) -> Self {
        Self { start, end }
    }

    pub fn range(&self) -> (BlockNumberOrTag, Option<BlockNumberOrTag>) {
        (self.start, self.end)
    }
}

impl Display for BlockRange {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}