use alloy::primitives::{Address, Bytes, B256, U256};
use anyhow::Result;
use serde_json::Value;
use std::str::FromStr;

const PORTAL_BASE_URL: &str = "https://portal.sqd.dev/datasets";

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

        // Find the highest block number in this page to continue from
        let last_block_num = page
            .iter()
            .filter_map(|b| b.get("header").and_then(|h| h.get("number")).and_then(|n| n.as_u64()))
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
    v.as_bool()
        .or_else(|| v.as_u64().map(|n| n != 0))
}

/// Parse a JSON value as u8 from integer.
pub fn value_to_u8(v: &Value) -> Option<u8> {
    v.as_u64().map(|n| n as u8)
}
