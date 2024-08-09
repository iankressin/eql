use super::{chain::Chain, ens::NameOrAddress};
use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, FixedBytes},
    providers::ProviderBuilder,
};
use std::{error::Error, fmt::Display, str::FromStr};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EntityId {
    Block(BlockRange),
    Transaction(FixedBytes<32>),
    Account(NameOrAddress),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BlockRange {
    start: BlockNumberOrTag,
    end: Option<BlockNumberOrTag>,
}

// TODO: return instance of Error trait instead of &'static str
impl TryFrom<&str> for EntityId {
    type Error = &'static str;

    fn try_from(id: &str) -> Result<Self, Self::Error> {
        if id.starts_with("0x") {
            if id.len() == 42 {
                let address = Address::from_str(id).map_err(|_| "Invalid address")?;
                let address = NameOrAddress::Address(address);
                Ok(EntityId::Account(address))
            } else if id.len() == 66 {
                let tx_hash = FixedBytes::from_str(id).map_err(|_| "Invalid tx hash")?;
                Ok(EntityId::Transaction(tx_hash))
            } else {
                // Return error: type not supported
                Err("Type not supported")
            }
        } else if id.ends_with(".eth") {
            let ens = NameOrAddress::Name(id.to_string());
            Ok(EntityId::Account(ens))
        } else {
            let (start, end) = match id.split_once(":") {
                Some((start, end)) => {
                    let start = start
                        .parse::<BlockNumberOrTag>()
                        .map_err(|_| "Invalid block number")?;
                    let end = end
                        .parse::<BlockNumberOrTag>()
                        .map_err(|_| "Invalid block number")?;
                    (start, Some(end))
                }
                None => (
                    id.parse::<BlockNumberOrTag>()
                        .map_err(|_| "Invalid block number")?,
                    None,
                ),
            };

            Ok(EntityId::Block(BlockRange { start, end }))
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EntityIdError {
    #[error("Invalid address")]
    InvalidAddress,
    #[error("Invalid tx hash")]
    InvalidTxHash,
    #[error("Invalid block number")]
    InvalidBlockNumber,
    #[error("Unable resolve ENS name")]
    EnsResolution,
}

impl EntityId {
    pub fn to_block_range(
        &self,
    ) -> Result<(BlockNumberOrTag, Option<BlockNumberOrTag>), EntityIdError> {
        match self {
            EntityId::Block(block_id) => Ok((block_id.start.clone(), block_id.end.clone())),
            _ => Err(EntityIdError::InvalidBlockNumber),
        }
    }

    pub fn to_tx_hash(&self) -> Result<FixedBytes<32>, EntityIdError> {
        match self {
            EntityId::Transaction(tx_hash) => Ok(*tx_hash),
            _ => Err(EntityIdError::InvalidTxHash),
        }
    }

    pub async fn to_address(&self) -> Result<Address, EntityIdError> {
        match self {
            EntityId::Account(name_or_address) => match &name_or_address {
                NameOrAddress::Address(address) => Ok(*address),
                NameOrAddress::Name(_) => {
                    let rpc_url = Chain::Ethereum
                        .rpc_url()
                        .parse()
                        .map_err(|_| EntityIdError::EnsResolution)?;

                    let provider = ProviderBuilder::new().on_http(rpc_url);

                    let address = name_or_address
                        .resolve(&provider)
                        .await
                        .map_err(|_| EntityIdError::EnsResolution)?;

                    Ok(address)
                }
            },
            _ => Err(EntityIdError::InvalidAddress),
        }
    }
}
