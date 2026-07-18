use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, Bloom, Bytes, B256, U256};
use anyhow::Result;
use serde_json::Value;
use std::str::FromStr;

use crate::common::block::{BlockId, BlockRange, BlockRangeError};

const PORTAL_BASE_URL: &str = "https://portal.sqd.dev/datasets";

fn next_portal_page_start(
    page: &[Value],
    current_from_block: u64,
    to_block: u64,
) -> Result<Option<u64>> {
    let last_block = page
        .iter()
        .filter_map(|block| {
            block
                .get("header")
                .and_then(|header| header.get("number"))
                .and_then(value_to_u64)
        })
        .max()
        .ok_or_else(|| {
            anyhow::anyhow!("Portal returned a nonempty page without a usable header.number")
        })?;

    if last_block < current_from_block {
        return Err(anyhow::anyhow!(
            "Portal pagination made no forward progress: last header.number {} is below requested fromBlock {}",
            last_block,
            current_from_block
        ));
    }

    Ok((last_block < to_block).then_some(last_block + 1))
}

/// Send a query to the SQD Portal stream API and return parsed NDJSON response blocks.
/// Automatically paginates by advancing `fromBlock` past the last returned block header
/// until the full requested range is covered.
pub async fn portal_query(dataset: &str, query: &Value) -> Result<Vec<Value>> {
    portal_query_with_base_url(PORTAL_BASE_URL, dataset, query).await
}

pub(crate) async fn portal_query_with_base_url(
    base_url: &str,
    dataset: &str,
    query: &Value,
) -> Result<Vec<Value>> {
    let url = format!("{}/{}/stream", base_url, dataset);
    let client = reqwest::Client::new();

    let to_block = query
        .get("toBlock")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);
    let mut current_from_block = query
        .get("fromBlock")
        .and_then(value_to_u64)
        .unwrap_or_default();

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

        let next_page_start = next_portal_page_start(&page, current_from_block, to_block)?;

        all_blocks.extend(page);

        match next_page_start {
            Some(next_block) => {
                // Advance fromBlock past the last returned block for the next page
                current_from_block = next_block;
                current_query
                    .as_object_mut()
                    .unwrap()
                    .insert("fromBlock".into(), Value::from(next_block));
            }
            _ => break, // Reached or exceeded toBlock
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
    v.as_bool().or_else(|| {
        value_to_u64(v).and_then(|n| match n {
            0 | 27 => Some(false),
            1 | 28 => Some(true),
            35.. => Some((n - 35) % 2 != 0),
            _ => None,
        })
    })
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

/// Resolve and validate a Portal block range after tags become concrete numbers.
pub async fn resolve_portal_range(dataset: &str, range: &BlockRange) -> Result<(u64, u64)> {
    let start = resolve_portal_bound(dataset, &range.start()).await?;
    let end = match range.end() {
        Some(end) => resolve_portal_bound(dataset, &end).await?,
        None => start,
    };

    if start > end {
        return Err(BlockRangeError::StartBlockMustBeLessThanEndBlock.into());
    }

    Ok((start, end))
}

/// Resolve a BlockId to a concrete (fromBlock, toBlock) range via Portal.
pub async fn resolve_block_id_range(dataset: &str, id: &BlockId) -> Result<(u64, u64)> {
    match id {
        BlockId::Number(t) => {
            let n = resolve_portal_bound(dataset, t).await?;
            Ok((n, n))
        }
        BlockId::Range(range) => resolve_portal_range(dataset, range).await,
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use serde_json::Value;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::{TcpListener, TcpStream},
        sync::{Arc, Mutex},
        thread::{self, JoinHandle},
    };

    pub(crate) fn spawn_mock_portal(
        responses: Vec<String>,
    ) -> (String, Arc<Mutex<Vec<Value>>>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock Portal");
        let address = listener.local_addr().expect("read mock Portal address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured_requests = Arc::clone(&requests);

        let handle = thread::spawn(move || {
            for response_body in responses {
                let (mut stream, _) = listener.accept().expect("accept mock Portal request");
                let request = read_json_request(&stream);
                captured_requests
                    .lock()
                    .expect("lock captured requests")
                    .push(request);

                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                )
                .expect("write mock Portal response");
            }
        });

        (format!("http://{address}"), requests, handle)
    }

    fn read_json_request(stream: &TcpStream) -> Value {
        let mut reader = BufReader::new(stream.try_clone().expect("clone mock Portal stream"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("read request line");

        let mut content_length = None;
        loop {
            line.clear();
            reader.read_line(&mut line).expect("read request header");
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    content_length =
                        Some(value.trim().parse::<usize>().expect("valid Content-Length"));
                }
            }
        }

        let mut body = vec![0; content_length.expect("request Content-Length")];
        reader.read_exact(&mut body).expect("read request body");
        serde_json::from_slice(&body).expect("parse request JSON")
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

        assert_eq!(next_portal_page_start(&page, 42, 43).unwrap(), Some(43));
    }

    #[test]
    fn test_next_portal_page_start_rejects_non_forward_page() {
        let page = vec![json!({ "header": { "number": "0x29" } })];

        let error = next_portal_page_start(&page, 42, 50)
            .expect_err("a page ending before its requested range must not paginate backward");

        assert_eq!(
            error.to_string(),
            "Portal pagination made no forward progress: last header.number 41 is below requested fromBlock 42"
        );
    }

    #[test]
    fn test_next_portal_page_start_advances_and_terminates_normally() {
        let first_page = vec![json!({ "header": { "number": "0x2a" } })];
        let last_page = vec![json!({ "header": { "number": "0x2b" } })];

        assert_eq!(
            next_portal_page_start(&first_page, 42, 43).unwrap(),
            Some(43)
        );
        assert_eq!(next_portal_page_start(&last_page, 43, 43).unwrap(), None);
    }

    #[tokio::test]
    async fn test_nonempty_page_without_header_number_is_an_error() {
        let (base_url, _requests, handle) = test_support::spawn_mock_portal(vec![
            "{\"transactions\":[{\"hash\":\"0x0000000000000000000000000000000000000000000000000000000000000001\"}]}\n"
                .to_string(),
        ]);
        let query = json!({
            "type": "evm",
            "fromBlock": 1,
            "toBlock": 2,
            "transactions": [{}],
            "fields": { "transaction": { "hash": true } }
        });

        let error = portal_query_with_base_url(&base_url, "test", &query)
            .await
            .expect_err("nonempty pages require pagination metadata");
        handle.join().expect("mock Portal thread");

        assert_eq!(
            error.to_string(),
            "Portal returned a nonempty page without a usable header.number"
        );
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
        assert_eq!(value_to_parity_bool(&json!(27)), Some(false));
        assert_eq!(value_to_parity_bool(&json!(28)), Some(true));
        assert_eq!(value_to_parity_bool(&json!(37)), Some(false));
        assert_eq!(value_to_parity_bool(&json!(38)), Some(true));
        assert_eq!(value_to_parity_bool(&json!("0x0")), Some(false));
        assert_eq!(value_to_parity_bool(&json!("0x1")), Some(true));
        assert_eq!(value_to_parity_bool(&json!("0x1b")), Some(false));
        assert_eq!(value_to_parity_bool(&json!("0x1c")), Some(true));
        assert_eq!(value_to_parity_bool(&json!("0x25")), Some(false));
        assert_eq!(value_to_parity_bool(&json!("0x26")), Some(true));
        assert_eq!(value_to_parity_bool(&json!(false)), Some(false));
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

    #[tokio::test]
    async fn test_resolve_block_id_range_rejects_reversed_tagged_range() {
        let id = BlockId::Range(BlockRange::new(
            BlockNumberOrTag::Number(10),
            Some(BlockNumberOrTag::Earliest),
        ));

        let error = resolve_block_id_range("unused", &id)
            .await
            .expect_err("resolved start must not exceed resolved end");
        assert_eq!(error.to_string(), "Start block must be less than end block");
    }

    #[tokio::test]
    async fn test_resolve_block_id_range_omitted_end_is_one_resolved_block() {
        let id = BlockId::Range(BlockRange::new(BlockNumberOrTag::Earliest, None));

        assert_eq!(resolve_block_id_range("unused", &id).await.unwrap(), (0, 0));
    }
}
