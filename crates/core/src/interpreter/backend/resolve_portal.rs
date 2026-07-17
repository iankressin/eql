use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, Bloom, Bytes, B256, U256};
use anyhow::Result;
use serde_json::Value;
use std::str::FromStr;

use crate::common::block::{BlockId, BlockRange};

const PORTAL_BASE_URL: &str = "https://portal.sqd.dev/datasets";

fn next_portal_page_start(page: &[Value], to_block: u64) -> Option<u64> {
    let last_block = page
        .iter()
        .filter_map(|block| {
            block
                .get("header")
                .and_then(|header| header.get("number"))
                .and_then(value_to_u64)
        })
        .max()?;

    (last_block < to_block).then_some(last_block + 1)
}

/// Send a query to the SQD Portal stream API and return parsed NDJSON response blocks.
/// Automatically paginates by advancing `fromBlock` past the last returned block header
/// until the full requested range is covered.
pub async fn portal_query(dataset: &str, query: &Value) -> Result<Vec<Value>> {
    let url = format!("{}/{}/stream", PORTAL_BASE_URL, dataset);
    let client = reqwest::Client::new();

    let to_block = query
        .get("toBlock")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);

    let mut all_blocks: Vec<Value> = Vec::new();
    let mut current_query = query.clone();

    loop {
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&current_query)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Portal request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Portal returned status {}: {}",
                status,
                body
            ));
        }

        let body = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read Portal response: {}", e))?;

        let page: Vec<Value> = body
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line))
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to parse Portal response: {}", e))?;

        if page.is_empty() {
            break;
        }

        let next_page_start = next_portal_page_start(&page, to_block);

        all_blocks.extend(page);

        match next_page_start {
            Some(next_block) => {
                // Advance fromBlock past the last returned block for the next page
                current_query
                    .as_object_mut()
                    .unwrap()
                    .insert("fromBlock".into(), Value::from(next_block));
            }
            _ => break, // Reached or exceeded toBlock, or no header numbers found
        }
    }

    Ok(all_blocks)
}

/// Parse a JSON value as u64 — handles both JSON integers and hex strings (e.g. "0xf7e9ab").
pub fn value_to_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| {
        v.as_str()
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
    })
}

/// Parse a JSON value as u128 — handles both JSON integers and hex strings.
pub fn value_to_u128(v: &Value) -> Option<u128> {
    v.as_u64().map(|n| n as u128).or_else(|| {
        v.as_str()
            .and_then(|s| u128::from_str_radix(s.trim_start_matches("0x"), 16).ok())
    })
}

/// Parse a JSON value as B256 from a hex string.
pub fn value_to_b256(v: &Value) -> Option<B256> {
    v.as_str().and_then(|s| B256::from_str(s).ok())
}

/// Parse a JSON value as Address from a hex string.
pub fn value_to_address(v: &Value) -> Option<Address> {
    v.as_str().and_then(|s| Address::from_str(s).ok())
}

/// Parse a JSON value as U256 from a hex string or integer.
pub fn value_to_u256(v: &Value) -> Option<U256> {
    if let Some(n) = v.as_u64() {
        return Some(U256::from(n));
    }
    v.as_str().and_then(|s| {
        if s.starts_with("0x") || s.starts_with("0X") {
            U256::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
        } else {
            U256::from_str(s).ok()
        }
    })
}

/// Parse a JSON value as Bytes from a hex string.
pub fn value_to_bytes(v: &Value) -> Option<Bytes> {
    v.as_str().and_then(|s| Bytes::from_str(s).ok())
}

/// Parse a JSON value as a boolean from integer, hex string, or bool.
pub fn value_to_status_bool(v: &Value) -> Option<bool> {
    value_to_u64(v).map(|n| n != 0).or_else(|| v.as_bool())
}

/// Parse a JSON value as u8 from integer or hex string.
pub fn value_to_u8(v: &Value) -> Option<u8> {
    value_to_u64(v).map(|n| n as u8)
}

/// Parse a JSON value as a Bloom from a hex string.
pub fn value_to_bloom(v: &Value) -> Option<Bloom> {
    v.as_str().and_then(|s| Bloom::from_str(s).ok())
}

/// Parse a JSON parity value (`v` / `yParity`) as a bool from int, hex string, or bool.
pub fn value_to_parity_bool(v: &Value) -> Option<bool> {
    value_to_u64(v).map(|n| n != 0).or_else(|| v.as_bool())
}

/// Returns true if a block tag can be resolved to a concrete number via Portal.
pub fn tag_is_portal_eligible(tag: &BlockNumberOrTag) -> bool {
    matches!(
        tag,
        BlockNumberOrTag::Number(_) | BlockNumberOrTag::Latest | BlockNumberOrTag::Earliest
    )
}

/// Returns true if both bounds of a BlockRange are Portal-resolvable tags.
pub fn block_range_is_portal_eligible(range: &BlockRange) -> bool {
    tag_is_portal_eligible(&range.start())
        && range.end().map_or(true, |e| tag_is_portal_eligible(&e))
}

/// Returns true if a BlockId is fully Portal-resolvable (concrete numbers / latest / earliest).
pub fn block_id_is_portal_eligible(id: &BlockId) -> bool {
    match id {
        BlockId::Number(t) => tag_is_portal_eligible(t),
        BlockId::Range(range) => block_range_is_portal_eligible(range),
    }
}

/// Fetch the current head block number for a dataset from Portal's `/head` endpoint.
pub async fn portal_head(dataset: &str) -> Result<u64> {
    let url = format!("{}/{}/head", PORTAL_BASE_URL, dataset);
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Portal /head request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Portal /head returned status {}: {}",
            status,
            body
        ));
    }

    let value: Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse Portal /head response: {}", e))?;

    value
        .get("number")
        .and_then(value_to_u64)
        .ok_or_else(|| anyhow::anyhow!("Portal /head response missing 'number'"))
}

/// Resolve a single block tag to a concrete number using Portal.
pub async fn resolve_portal_bound(dataset: &str, tag: &BlockNumberOrTag) -> Result<u64> {
    match tag {
        BlockNumberOrTag::Number(n) => Ok(*n),
        BlockNumberOrTag::Earliest => Ok(0),
        BlockNumberOrTag::Latest => portal_head(dataset).await,
        other => Err(anyhow::anyhow!(
            "Block tag {:?} cannot be resolved via Portal",
            other
        )),
    }
}

/// Resolve a BlockId to a concrete (fromBlock, toBlock) range via Portal.
pub async fn resolve_block_id_range(dataset: &str, id: &BlockId) -> Result<(u64, u64)> {
    match id {
        BlockId::Number(t) => {
            let n = resolve_portal_bound(dataset, t).await?;
            Ok((n, n))
        }
        BlockId::Range(range) => {
            let start = resolve_portal_bound(dataset, &range.start()).await?;
            let end = match range.end() {
                Some(e) => resolve_portal_bound(dataset, &e).await?,
                None => start,
            };
            Ok((start, end))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::eips::BlockNumberOrTag;
    use serde_json::json;

    #[test]
    fn test_next_portal_page_start_uses_hex_header_number() {
        let page = vec![json!({ "header": { "number": "0x2a" } })];

        assert_eq!(next_portal_page_start(&page, 43), Some(43));
    }

    #[test]
    fn test_value_to_bloom_parses_hex_string() {
        let zeros = format!("0x{}", "0".repeat(512));
        let bloom = value_to_bloom(&json!(zeros)).expect("should parse bloom");
        assert_eq!(bloom, Bloom::ZERO);
        assert!(value_to_bloom(&json!(123)).is_none());
    }

    #[test]
    fn test_value_to_parity_bool_handles_int_and_hex() {
        assert_eq!(value_to_parity_bool(&json!(0)), Some(false));
        assert_eq!(value_to_parity_bool(&json!(1)), Some(true));
        assert_eq!(value_to_parity_bool(&json!("0x0")), Some(false));
        assert_eq!(value_to_parity_bool(&json!("0x1")), Some(true));
        assert_eq!(value_to_parity_bool(&json!(true)), Some(true));
    }

    #[test]
    fn test_value_to_status_bool_handles_int_and_hex() {
        assert_eq!(value_to_status_bool(&json!(0)), Some(false));
        assert_eq!(value_to_status_bool(&json!(1)), Some(true));
        assert_eq!(value_to_status_bool(&json!("0x0")), Some(false));
        assert_eq!(value_to_status_bool(&json!("0x1")), Some(true));
        assert_eq!(value_to_status_bool(&json!(true)), Some(true));
    }

    #[test]
    fn test_value_to_u8_handles_int_and_hex() {
        assert_eq!(value_to_u8(&json!(2)), Some(2));
        assert_eq!(value_to_u8(&json!("0xff")), Some(255));
    }

    #[test]
    fn test_tag_eligibility() {
        assert!(tag_is_portal_eligible(&BlockNumberOrTag::Number(5)));
        assert!(tag_is_portal_eligible(&BlockNumberOrTag::Latest));
        assert!(tag_is_portal_eligible(&BlockNumberOrTag::Earliest));
        assert!(!tag_is_portal_eligible(&BlockNumberOrTag::Pending));
        assert!(!tag_is_portal_eligible(&BlockNumberOrTag::Finalized));
        assert!(!tag_is_portal_eligible(&BlockNumberOrTag::Safe));
    }
}
