use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, Bloom, Bytes, B256, U256};
use anyhow::Result;
use serde_json::{json, Value};
use std::str::FromStr;

use crate::common::block::{BlockId, BlockRange};

const PORTAL_BASE_URL: &str = "https://portal.sqd.dev/datasets";

/// Ensure a Portal query requests `fields.block.number`, merging with whatever `fields` /
/// `fields.block` selection is already present rather than clobbering it.
///
/// `portal_query` paginates by reading `header.number` off every returned block to advance
/// `fromBlock`. Callers that only select e.g. `fields.transaction` (transaction queries) or
/// `fields.log` (log queries) never asked for block headers, so Portal would return
/// `"header": {}` and pagination would silently stop after the first chunk, truncating
/// multi-chunk ranges. Forcing `block.number` here — inside `portal_query` itself — makes the
/// invariant hold for every caller regardless of what fields they request.
fn ensure_block_number_selected(query: &Value) -> Value {
    let mut query = query.clone();

    let Some(obj) = query.as_object_mut() else {
        // Not a JSON object; nothing sensible to merge into. Internally-constructed Portal
        // queries are always objects, so this only guards against misuse.
        return query;
    };

    let fields = obj.entry("fields").or_insert_with(|| json!({}));
    if !fields.is_object() {
        *fields = json!({});
    }
    let fields_obj = fields
        .as_object_mut()
        .expect("fields was just normalized to an object");

    let block = fields_obj.entry("block").or_insert_with(|| json!({}));
    if !block.is_object() {
        *block = json!({});
    }
    let block_obj = block
        .as_object_mut()
        .expect("block was just normalized to an object");

    block_obj.insert("number".into(), json!(true));

    query
}

/// Send a query to the SQD Portal stream API and return parsed NDJSON response blocks.
/// Automatically paginates by advancing `fromBlock` past the last returned block header
/// until the full requested range is covered.
///
/// Forces `fields.block.number` on (merging with the caller's field selection) so
/// pagination always has a block number to advance from, even for transaction/log queries
/// that don't otherwise select any block-header fields.
pub async fn portal_query(dataset: &str, query: &Value) -> Result<Vec<Value>> {
    let url = format!("{}/{}/stream", PORTAL_BASE_URL, dataset);
    let client = reqwest::Client::new();

    let to_block = query
        .get("toBlock")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);

    let mut all_blocks: Vec<Value> = Vec::new();
    let mut current_query = ensure_block_number_selected(query);

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

        // Find the highest block number in this page to continue from
        let last_block_num = page
            .iter()
            .filter_map(|b| {
                b.get("header")
                    .and_then(|h| h.get("number"))
                    .and_then(|n| n.as_u64())
            })
            .max();

        all_blocks.extend(page);

        match last_block_num {
            Some(last) if last < to_block => {
                // Advance fromBlock past the last returned block for the next page
                current_query
                    .as_object_mut()
                    .unwrap()
                    .insert("fromBlock".into(), Value::from(last + 1));
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

/// Parse a JSON value as a boolean from integer (0/1) or bool.
pub fn value_to_status_bool(v: &Value) -> Option<bool> {
    v.as_bool().or_else(|| v.as_u64().map(|n| n != 0))
}

/// Parse a JSON value as u8 from integer.
pub fn value_to_u8(v: &Value) -> Option<u8> {
    v.as_u64().map(|n| n as u8)
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
    fn test_ensure_block_number_selected_adds_to_empty_fields() {
        let query = json!({"type": "evm", "fromBlock": 1});
        let result = ensure_block_number_selected(&query);
        assert_eq!(result["fields"]["block"]["number"], json!(true));
    }

    #[test]
    fn test_ensure_block_number_selected_preserves_existing_block_fields() {
        let query = json!({
            "fields": { "block": { "timestamp": true, "hash": true } }
        });
        let result = ensure_block_number_selected(&query);
        assert_eq!(result["fields"]["block"]["number"], json!(true));
        assert_eq!(result["fields"]["block"]["timestamp"], json!(true));
        assert_eq!(result["fields"]["block"]["hash"], json!(true));
    }

    #[test]
    fn test_ensure_block_number_selected_is_idempotent() {
        let query = json!({
            "fields": { "block": { "number": true } }
        });
        let result = ensure_block_number_selected(&query);
        assert_eq!(result["fields"]["block"]["number"], json!(true));
        assert_eq!(result["fields"]["block"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn test_ensure_block_number_selected_preserves_transaction_fields() {
        let query = json!({
            "fields": { "transaction": { "hash": true } }
        });
        let result = ensure_block_number_selected(&query);
        assert_eq!(result["fields"]["transaction"]["hash"], json!(true));
        assert_eq!(
            result["fields"]["transaction"].as_object().unwrap().len(),
            1
        );
        assert_eq!(result["fields"]["block"]["number"], json!(true));
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
    fn test_tag_eligibility() {
        assert!(tag_is_portal_eligible(&BlockNumberOrTag::Number(5)));
        assert!(tag_is_portal_eligible(&BlockNumberOrTag::Latest));
        assert!(tag_is_portal_eligible(&BlockNumberOrTag::Earliest));
        assert!(!tag_is_portal_eligible(&BlockNumberOrTag::Pending));
        assert!(!tag_is_portal_eligible(&BlockNumberOrTag::Finalized));
        assert!(!tag_is_portal_eligible(&BlockNumberOrTag::Safe));
    }
}
