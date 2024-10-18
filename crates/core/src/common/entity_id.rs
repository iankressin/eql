use alloy::eips::BlockNumberOrTag;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EntityIdError {
    #[error("Invalid address")]
    InvalidAddress,
    #[error("Invalid tx hash")]
    InvalidTxHash,
    #[error("Invalid block number or tag: {0}")]
    InvalidBlockNumberOrTag(String),
    #[error("Unable resolve ENS name")]
    EnsResolution,
}

pub fn parse_block_number_or_tag(id: &str) -> Result<BlockNumberOrTag, EntityIdError> {
    match id.trim().parse::<u64>() {
        Ok(id) => Ok(BlockNumberOrTag::Number(id)),
        Err(_) => id
            .parse::<BlockNumberOrTag>()
            .map_err(|_| EntityIdError::InvalidBlockNumberOrTag(id.to_string())),
    }
}
