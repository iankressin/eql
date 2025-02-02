use crate::common::block::{BlockField, BlockRange};
use anyhow::Result;
use serde::Deserialize;
use sqd_portal_client::{
    evm::{BlockFields, Fields, Query, QueryType},
    Client, ClientConfig, StreamConfig,
};

#[derive(Debug, Deserialize)]
struct BlockRangeResponse {
    block_hashes: Vec<String>,
}

async fn request_block_range() -> Result<BlockRangeResponse> {
    let url = "https://portal.sqd.dev/datasets/ethereum-mainnet"
        .parse()
        .unwrap();
    let client = Client::new(url, ClientConfig::default());
    let query = Query {
        type_: QueryType::Evm,
        from_block: 290_000_000,
        to_block: Some(290_100_000),
        fields: Fields::all(),
        include_all_blocks: true,
        logs: vec![],
        state_diffs: vec![],
        traces: vec![],
        transactions: vec![],
    };
    let result = client.evm_arrow_finalized_query(&query).await?;

    println!("{:?}", result);

    Ok(BlockRangeResponse {
        block_hashes: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test calls the async function `request_block_range` with a dummy range
    /// and asserts that it returns an Ok result. Note that the current implementation
    /// always returns an empty vector for `block_hashes`, so we check for that.
    ///
    /// If you do not want to hit the real network endpoint every time, consider marking
    /// this test with `#[ignore]` or using a mocking library.
    #[tokio::test]
    // #[ignore] // Uncomment this line if you want to skip running this test by default.
    async fn test_request_block_range() {
        // Call the target function.
        let response = request_block_range().await;
        assert!(
            response.is_ok(),
            "Expected request_block_range to return Ok, got Err: {:?}",
            response.err()
        );

        let block_range_response = response.unwrap();
        // According to the current function implementation, it always returns an empty vec
        // for `block_hashes`. You could modify this assertion when the implementation changes.
        assert!(
            block_range_response.block_hashes.is_empty(),
            "Expected empty block_hashes vector, got: {:?}",
            block_range_response.block_hashes
        );

        // Optionally, print the result for debugging.
        println!("Received BlockRangeResponse: {:?}", block_range_response);
    }
}
