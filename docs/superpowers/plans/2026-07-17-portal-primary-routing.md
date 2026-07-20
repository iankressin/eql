# Portal-Primary Block/Transaction/Log Routing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route all block/transaction/log field selections (including `GET *`) through SQD Portal, leaving RPC only for cases Portal structurally cannot serve.

**Architecture:** Complete the per-resolver Portal field mappings + parse-back so every EQL field is Portal-serviceable (some via correct defaults), then delete the field-coverage routing gate — routing becomes purely query-shape based. Add shared Portal primitives (bloom decode, `/head`, tag→number resolution) so `latest`/`earliest` resolve via Portal. Fix Fantom's dead dataset mapping. *(Superseded: Fantom was later removed as a chain entirely — see `2026-07-17-remove-fantom-support.md`.)*

**Tech Stack:** Rust (Cargo workspace), `alloy 0.6.4`, `alloy-eip7702 0.4.1`, `reqwest`, `serde_json`, `anyhow`, `pest`. Tests are in-module `#[cfg(test)]`; CI runs plain `cargo test` (network-capable — existing e2e tests hit live Ethereum/Portal).

**Design doc:** `docs/superpowers/specs/2026-07-17-portal-primary-routing-design.md`

## Global Constraints

- All work is in `crates/core`. Run tests with `cargo test -p eql_core`.
- Portal base URL is `https://portal.sqd.dev/datasets` (`resolve_portal.rs:6`, `PORTAL_BASE_URL`).
- Portal serializes numeric header/tx fields as **hex strings** (e.g. `"0x7ff800000"`) OR JSON integers depending on field; always decode with the hex-aware `value_to_*` helpers, never `as_u64()` directly.
- Parse-back and field-name `match` blocks must be **exhaustive (no `_ => {}` wildcard)** so the compiler forces every future enum variant to be handled — this is the primary defense against the allowlist drift that caused this bug.
- Preserve existing behavior for the true fallback cases: `account` entity, transaction-by-hash, `block_hash` log filter, `pending`/`finalized`/`safe` tags, and chains without a Portal dataset (`ronin`/`kava`/`mekong`) all stay on RPC. *(`fantom` was originally in this list; it was later removed as a chain entirely — see `2026-07-17-remove-fantom-support.md`.)*
- Decisions (from spec §3): `authorization_list` → `None` on Portal; `latest`→`/head`, `earliest`→`0`; log `removed`→`Some(false)`; log `block_hash`→from header; `EventSignature` filter → `topic0 = keccak256(sig)`.

---

## Task 1: Shared Portal routing primitives

**Files:**
- Modify: `crates/core/src/interpreter/backend/resolve_portal.rs`
- Test: `crates/core/src/interpreter/backend/resolve_portal.rs` (`#[cfg(test)]` module at end)

**Interfaces:**
- Produces (all `pub`, used by Tasks 3/4/5):
  - `fn value_to_bloom(v: &Value) -> Option<Bloom>`
  - `fn value_to_parity_bool(v: &Value) -> Option<bool>`
  - `async fn portal_head(dataset: &str) -> Result<u64>`
  - `async fn resolve_portal_bound(dataset: &str, tag: &BlockNumberOrTag) -> Result<u64>`
  - `fn tag_is_portal_eligible(tag: &BlockNumberOrTag) -> bool`
  - `fn block_range_is_portal_eligible(range: &BlockRange) -> bool`
  - `fn block_id_is_portal_eligible(id: &BlockId) -> bool`
  - `async fn resolve_block_id_range(dataset: &str, id: &BlockId) -> Result<(u64, u64)>`

- [ ] **Step 1: Write failing tests**

Add at the bottom of `resolve_portal.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloy::eips::BlockNumberOrTag;
    use serde_json::json;

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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core resolve_portal::tests -- --nocapture`
Expected: FAIL — `value_to_bloom`, `value_to_parity_bool`, `tag_is_portal_eligible` not found.

- [ ] **Step 3: Implement the primitives**

At the top of `resolve_portal.rs`, extend the imports:

```rust
use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, Bloom, Bytes, B256, U256};
use anyhow::Result;
use serde_json::Value;
use std::str::FromStr;

use crate::common::block::{BlockId, BlockRange};
```

Add these functions (after the existing `value_to_*` helpers):

```rust
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
    tag_is_portal_eligible(&range.start()) && range.end().map_or(true, |e| tag_is_portal_eligible(&e))
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
        return Err(anyhow::anyhow!("Portal /head returned status {}: {}", status, body));
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core resolve_portal::tests`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/interpreter/backend/resolve_portal.rs
git commit -m "feat(portal): add bloom/parity decoders and tag-resolution primitives"
```

---

## Task 2: Fantom → RPC (fix dead Portal dataset)

> **SUPERSEDED — do not execute.** This task kept `Chain::Fantom` and routed it to RPC. The product decision changed to removing Fantom entirely, and `Chain::Fantom` no longer exists. See `docs/superpowers/plans/2026-07-17-remove-fantom-support.md` and `docs/superpowers/specs/2026-07-17-remove-fantom-support-design.md`.

**Files:**
- Modify: `crates/core/src/common/chain.rs:112`
- Test: `crates/core/src/common/chain.rs` (new `#[cfg(test)]` module at end)

**Interfaces:**
- Consumes: nothing. Produces: `Chain::Fantom.portal_dataset()` now returns `None`.

- [ ] **Step 1: Write the failing test**

Append to `chain.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fantom_has_no_portal_dataset() {
        // fantom-mainnet 404s on Portal (Fantom Opera dataset dropped); must fall back to RPC.
        assert_eq!(Chain::Fantom.portal_dataset(), None);
    }

    #[test]
    fn test_supported_chain_still_has_dataset() {
        assert_eq!(Chain::Ethereum.portal_dataset(), Some("ethereum-mainnet"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p eql_core chain::tests::test_fantom_has_no_portal_dataset`
Expected: FAIL — returns `Some("fantom-mainnet")`, not `None`.

- [ ] **Step 3: Change the mapping**

In `chain.rs`, in `portal_dataset()`, change:

```rust
            Chain::Fantom => Some("fantom-mainnet"),
```

to:

```rust
            Chain::Fantom => None,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core chain::tests`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/common/chain.rs
git commit -m "fix(portal): route Fantom to RPC (fantom-mainnet dataset is gone)"
```

---

## Task 3: Block — full Portal field coverage + tag resolution

**Files:**
- Modify: `crates/core/src/interpreter/backend/resolve_block.rs`
- Test: same file (`#[cfg(test)]` module) + existing e2e `execution_engine.rs::test_get_block_fields` (verify, no change)

**Interfaces:**
- Consumes (Task 1): `value_to_bloom`, `value_to_u256`, `value_to_bytes`, `block_id_is_portal_eligible`, `resolve_block_id_range`.
- Produces: `resolve_block_query` routes every `BlockField` (incl. `GET *`) and `Number`/`latest`/`earliest` ids through Portal.

- [ ] **Step 1: Write the failing unit test**

Add to the `#[cfg(test)] mod tests` in `resolve_block.rs`:

```rust
    #[test]
    fn test_parse_portal_block_header_decodes_all_fields() {
        use serde_json::json;
        let header = json!({
            "number": 1,
            "timestamp": 1438269988u64,
            "hash": "0x88e96d4537bea4d9c05d12549907b32561d3bf31f45aae734cdc119f13406cb6",
            "size": 537,
            "totalDifficulty": "0x7ff800000",
            "baseFeePerGas": null
        });
        let fields = vec![
            BlockField::Number,
            BlockField::Size,
            BlockField::TotalDifficulty,
            BlockField::BaseFeePerGas,
        ];
        let res = parse_portal_block_header(&header, &fields, &Chain::Ethereum);
        assert_eq!(res.number, Some(1));
        assert_eq!(res.size, Some(alloy::primitives::U256::from(537)));
        assert_eq!(res.total_difficulty, Some(alloy::primitives::U256::from(34351349760u64)));
        assert_eq!(res.base_fee_per_gas, None);
    }

    #[test]
    fn test_block_field_mapping_is_exhaustive() {
        // all_variants() returns &'static [BlockField], so `field` is already &BlockField.
        for field in BlockField::all_variants() {
            let mapped = block_field_to_portal_name(field).is_some();
            let local = matches!(field, BlockField::Chain);
            assert!(mapped || local, "BlockField {:?} not Portal-serviceable", field);
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core resolve_block::tests`
Expected: FAIL — `size`/`total_difficulty` unmapped (parse returns `None`), assertion fails.

- [ ] **Step 3: Extend imports + field mapping + parse-back; remove field gate**

Change the import on line 1 from:

```rust
use super::resolve_portal::{portal_query, value_to_b256, value_to_u64};
```

to:

```rust
use super::resolve_portal::{
    block_id_is_portal_eligible, portal_query, resolve_block_id_range, value_to_b256,
    value_to_bloom, value_to_bytes, value_to_u256, value_to_u64,
};
```

Delete `field_supported_by_portal` (lines 30-43) and `block_id_is_concrete` (lines 46-57).

Replace `should_use_portal` (60-69) with:

```rust
/// Determines if a block query for a given chain should use the Portal.
fn should_use_portal(chain: &ChainOrRpc, ids: &[BlockId]) -> bool {
    let dataset = match chain {
        ChainOrRpc::Chain(c) => c.portal_dataset(),
        ChainOrRpc::Rpc(_) => None,
    };
    dataset.is_some() && ids.iter().all(block_id_is_portal_eligible)
}
```

Update the call site (line 99): `if should_use_portal(chain, ids) {`.

In `resolve_blocks_via_portal`, replace the `let (from_block, to_block) = block_id_to_range(id);` line (129) with:

```rust
        let (from_block, to_block) = resolve_block_id_range(dataset, id).await?;
```

Delete the now-unused `block_id_to_range` function (166-183).

Extend `block_field_to_portal_name` (186-198) to be exhaustive:

```rust
fn block_field_to_portal_name(field: &BlockField) -> Option<&'static str> {
    match field {
        BlockField::Number => Some("number"),
        BlockField::Timestamp => Some("timestamp"),
        BlockField::Hash => Some("hash"),
        BlockField::ParentHash => Some("parentHash"),
        BlockField::StateRoot => Some("stateRoot"),
        BlockField::TransactionsRoot => Some("transactionsRoot"),
        BlockField::ReceiptsRoot => Some("receiptsRoot"),
        BlockField::BaseFeePerGas => Some("baseFeePerGas"),
        BlockField::Size => Some("size"),
        BlockField::LogsBloom => Some("logsBloom"),
        BlockField::ExtraData => Some("extraData"),
        BlockField::MixHash => Some("mixHash"),
        BlockField::TotalDifficulty => Some("totalDifficulty"),
        BlockField::WithdrawalsRoot => Some("withdrawalsRoot"),
        BlockField::BlobGasUsed => Some("blobGasUsed"),
        BlockField::ExcessBlobGas => Some("excessBlobGas"),
        BlockField::ParentBeaconBlockRoot => Some("parentBeaconBlockRoot"),
        BlockField::Chain => None,
    }
}
```

Extend `parse_portal_block_header` (201-243) — replace the `_ => {}` arm with the nine new arms so the match is exhaustive:

```rust
            BlockField::Size => {
                result.size = header.get("size").and_then(value_to_u256);
            }
            BlockField::LogsBloom => {
                result.logs_bloom = header.get("logsBloom").and_then(value_to_bloom);
            }
            BlockField::ExtraData => {
                result.extra_data = header.get("extraData").and_then(value_to_bytes);
            }
            BlockField::MixHash => {
                result.mix_hash = header.get("mixHash").and_then(value_to_b256);
            }
            BlockField::TotalDifficulty => {
                result.total_difficulty = header.get("totalDifficulty").and_then(value_to_u256);
            }
            BlockField::WithdrawalsRoot => {
                result.withdrawals_root = header.get("withdrawalsRoot").and_then(value_to_b256);
            }
            BlockField::BlobGasUsed => {
                result.blob_gas_used = header.get("blobGasUsed").and_then(value_to_u64);
            }
            BlockField::ExcessBlobGas => {
                result.excess_blob_gas = header.get("excessBlobGas").and_then(value_to_u64);
            }
            BlockField::ParentBeaconBlockRoot => {
                result.parent_beacon_block_root =
                    header.get("parentBeaconBlockRoot").and_then(value_to_b256);
            }
```

(Keep the existing arms for Number/Timestamp/Hash/ParentHash/StateRoot/TransactionsRoot/ReceiptsRoot/BaseFeePerGas/Chain.)

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cargo test -p eql_core resolve_block::tests`
Expected: PASS.

- [ ] **Step 5: Run the e2e GET * block test (now routes via Portal) to verify parity**

Run: `cargo test -p eql_core test_get_block_fields`
Expected: PASS unchanged — verified: Portal returns block 1 with `size:537`, `totalDifficulty:"0x7ff800000"` (→ `U256::from(34351349760)`), post-merge fields `null` (→ `None`).

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/interpreter/backend/resolve_block.rs
git commit -m "feat(portal): serve all block fields via Portal + resolve latest/earliest"
```

---

## Task 4: Transaction — full Portal field coverage + tag resolution

**Files:**
- Modify: `crates/core/src/interpreter/backend/resolve_transaction.rs`
- Test: same file + new e2e test in `crates/core/src/interpreter/backend/execution_engine.rs`

**Interfaces:**
- Consumes (Task 1): `value_to_parity_bool`, `value_to_u256`, `value_to_u128`, `block_id_is_portal_eligible`, `resolve_block_id_range`.
- Produces: block-range transaction queries serve all `TransactionField`s (incl. `GET *`) via Portal; `authorization_list` is `None` on the Portal path; by-hash queries remain RPC.

- [ ] **Step 1: Write the failing unit test**

Add to the `#[cfg(test)] mod tests` in `resolve_transaction.rs`:

```rust
    #[test]
    fn test_parse_portal_transaction_decodes_signature_fields() {
        use serde_json::json;
        let tx = json!({
            "effectiveGasPrice": 10209184711u64,
            "v": "0x0",
            "yParity": "0x0",
            "maxFeePerBlobGas": null
        });
        let fields = vec![
            TransactionField::EffectiveGasPrice,
            TransactionField::V,
            TransactionField::YParity,
            TransactionField::MaxFeePerBlobGas,
            TransactionField::AuthorizationList,
        ];
        let res = parse_portal_transaction(&tx, &fields, &Chain::Ethereum);
        assert_eq!(res.effective_gas_price, Some(10209184711u128));
        assert_eq!(res.v, Some(false));
        assert_eq!(res.y_parity, Some(false));
        assert_eq!(res.max_fee_per_blob_gas, None);
        assert_eq!(res.authorization_list, None);
    }

    #[test]
    fn test_tx_field_mapping_is_exhaustive() {
        // all_variants() returns &'static [TransactionField]; `field` is already &TransactionField.
        for field in TransactionField::all_variants() {
            let mapped = tx_field_to_portal_name(field).is_some();
            let local = matches!(
                field,
                TransactionField::Chain | TransactionField::AuthorizationList
            );
            assert!(mapped || local, "TransactionField {:?} not Portal-serviceable", field);
        }
    }
```

(Add `use crate::common::chain::Chain;` to the test module imports if not present — `Chain` is already imported at file scope via `crate::common::chain::{Chain, ChainOrRpc}`, so `super::*` covers it.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core resolve_transaction::tests`
Expected: FAIL — new signature fields unmapped.

- [ ] **Step 3: Extend imports, mapping, parse-back; update the gate for tags**

Extend the import block (lines 2-5) to add `value_to_parity_bool`, `block_id_is_portal_eligible`, `resolve_block_id_range`:

```rust
use super::resolve_portal::{
    block_id_is_portal_eligible, portal_query, resolve_block_id_range, value_to_address,
    value_to_b256, value_to_bytes, value_to_parity_bool, value_to_status_bool, value_to_u128,
    value_to_u256, value_to_u64, value_to_u8,
};
```

Delete `field_supported_by_portal` (35-52) and `block_id_to_concrete_range` (55-72).

Rewrite `should_use_portal` (100-134) to drop the field check and use the shared tag-eligibility helper:

```rust
/// Determines if a transaction query for a given chain should use the Portal.
fn should_use_portal(chain: &ChainOrRpc, transaction: &Transaction) -> bool {
    let dataset = match chain {
        ChainOrRpc::Chain(c) => c.portal_dataset(),
        ChainOrRpc::Rpc(_) => None,
    };
    if dataset.is_none() {
        return false;
    }
    // Portal has no transaction-by-hash filter.
    if transaction.ids().is_some() {
        return false;
    }
    // Portal needs a block range to scan.
    if !transaction.has_block_filter() {
        return false;
    }
    match transaction.get_block_id_filter() {
        std::result::Result::Ok(id) => block_id_is_portal_eligible(id),
        Err(_) => false,
    }
}
```

In `resolve_transactions_via_portal`, replace the range extraction (173-174):

```rust
    let block_id = transaction.get_block_id_filter()?;
    let (from_block, to_block) = block_id_to_concrete_range(block_id).unwrap();
```

with:

```rust
    let block_id = transaction.get_block_id_filter()?;
    let (from_block, to_block) = resolve_block_id_range(dataset, block_id).await?;
```

Extend `tx_field_to_portal_name` (224-240) to be exhaustive:

```rust
fn tx_field_to_portal_name(field: &TransactionField) -> Option<&'static str> {
    match field {
        TransactionField::Type => Some("type"),
        TransactionField::Hash => Some("hash"),
        TransactionField::From => Some("from"),
        TransactionField::To => Some("to"),
        TransactionField::Data => Some("input"),
        TransactionField::Value => Some("value"),
        TransactionField::GasPrice => Some("gasPrice"),
        TransactionField::GasLimit => Some("gas"),
        TransactionField::Status => Some("status"),
        TransactionField::ChainId => Some("chainId"),
        TransactionField::MaxFeePerGas => Some("maxFeePerGas"),
        TransactionField::MaxPriorityFeePerGas => Some("maxPriorityFeePerGas"),
        TransactionField::EffectiveGasPrice => Some("effectiveGasPrice"),
        TransactionField::V => Some("v"),
        TransactionField::R => Some("r"),
        TransactionField::S => Some("s"),
        TransactionField::MaxFeePerBlobGas => Some("maxFeePerBlobGas"),
        TransactionField::YParity => Some("yParity"),
        // Not requested from Portal:
        TransactionField::Chain => None,            // set locally
        TransactionField::AuthorizationList => None, // no Portal field (EIP-7702)
    }
}
```

Extend `parse_portal_transaction` (243-297) — replace the `_ => {}` arm with the new arms so the match is exhaustive:

```rust
            TransactionField::EffectiveGasPrice => {
                result.effective_gas_price = tx.get("effectiveGasPrice").and_then(value_to_u128);
            }
            TransactionField::V => {
                result.v = tx.get("v").and_then(value_to_parity_bool);
            }
            TransactionField::R => {
                result.r = tx.get("r").and_then(value_to_u256);
            }
            TransactionField::S => {
                result.s = tx.get("s").and_then(value_to_u256);
            }
            TransactionField::MaxFeePerBlobGas => {
                result.max_fee_per_blob_gas = tx.get("maxFeePerBlobGas").and_then(value_to_u128);
            }
            TransactionField::YParity => {
                result.y_parity = tx.get("yParity").and_then(value_to_parity_bool);
            }
            TransactionField::AuthorizationList => {
                // Not available on Portal (EIP-7702); left as None. By-hash queries (RPC) fill it.
            }
```

> Implementation note: `v`/`y_parity` are `Option<bool>` (parity semantics). For legacy (type-0) txs Portal's `v` is `27/28` and `yParity` may be `null`, so both are lossy on the Portal path for legacy txs — matching EQL's pre-existing bool typing. The e2e test below uses type-2 txs where parity is cleanly `0/1`.

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cargo test -p eql_core resolve_transaction::tests`
Expected: PASS.

- [ ] **Step 5: Add an e2e test exercising the transaction Portal path (by block range)**

The existing `test_get_transaction_fields` is a by-hash query (stays RPC), so it does not cover the Portal path. Add this to `execution_engine.rs`'s `#[cfg(test)] mod test` (imports there already include `Transaction`, `TransactionField`, `TransactionFilter`, `BlockId`, `BlockRange`, `BlockNumberOrTag`):

```rust
    #[tokio::test]
    async fn test_get_transactions_via_portal_block_range() {
        use crate::common::transaction::TransactionFilter;
        use crate::common::filters::EqualityFilter;

        let execution_engine = ExecutionEngine::new();
        // A single concrete block, filtered by sender, GET * -> must route through Portal.
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                None,
                Some(vec![
                    TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                        BlockNumberOrTag::Number(20000000),
                        Some(BlockNumberOrTag::Number(20000000)),
                    ))),
                    TransactionFilter::From(EqualityFilter::Eq(address!(
                        "95222290dd7278aa3ddd389cc1e1d165cc4bafe5"
                    ))),
                ]),
                TransactionField::all_variants().to_vec(),
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
        })];

        let result = execution_engine.run(expressions).await.unwrap();
        match &result[0].result {
            ExpressionResult::Transaction(txs) => {
                assert!(!txs.is_empty(), "expected at least one tx from Portal");
                // authorization_list is always None on the Portal path.
                assert!(txs.iter().all(|t| t.authorization_list.is_none()));
                // GET * populates hash + from on every row.
                assert!(txs.iter().all(|t| t.hash.is_some() && t.from.is_some()));
            }
            other => panic!("expected Transaction result, got {:?}", other),
        }
    }
```

> Verified against Portal: block 20000000 contains exactly one transaction from `0x95222290dd7278aa3ddd389cc1e1d165cc4bafe5` (hash `0xb79b64182236284ad6753e1b5f506e7e6989912c25887575f82d64f23f6bf267`). Re-run this to re-confirm before locking constants:
> `curl -s -X POST https://portal.sqd.dev/datasets/ethereum-mainnet/stream -H 'Content-Type: application/json' -d '{"type":"evm","fromBlock":20000000,"toBlock":20000000,"transactions":[{"from":["0x95222290dd7278aa3ddd389cc1e1d165cc4bafe5"]}],"fields":{"transaction":{"hash":true,"from":true}}}'`

- [ ] **Step 6: Run the e2e test**

Run: `cargo test -p eql_core test_get_transactions_via_portal_block_range`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/interpreter/backend/resolve_transaction.rs crates/core/src/interpreter/backend/execution_engine.rs
git commit -m "feat(portal): serve all transaction fields via Portal for block-range queries"
```

---

## Task 5: Log — full Portal field coverage, EventSignature filter, tag resolution

**Files:**
- Modify: `crates/core/src/interpreter/backend/resolve_logs.rs`
- Test: same file + update existing e2e `execution_engine.rs::test_get_logs`

**Interfaces:**
- Consumes (Task 1): `value_to_b256`, `resolve_portal_bound`, `block_range_is_portal_eligible`.
- Produces: all `LogField`s served via Portal (`block_hash` from header, `removed`→`false`); `EventSignature` filter mapped to `topic0`; `latest`/`earliest` range bounds resolved via Portal; `block_hash` filter still forces RPC.

- [ ] **Step 1: Write the failing unit test**

Add to the `#[cfg(test)] mod tests` in `resolve_logs.rs` (create the module if absent — see structure below):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::chain::Chain;
    use alloy::primitives::b256;
    use serde_json::json;

    #[test]
    fn test_parse_portal_log_sets_block_hash_and_removed() {
        let log = json!({
            "logIndex": 5,
            "transactionIndex": 9,
            "address": "0xdac17f958d2ee523a2206206994597c13d831ec7",
            "topics": ["0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a"]
        });
        let fields = vec![LogField::BlockHash, LogField::Removed, LogField::LogIndex];
        let block_hash = Some(b256!(
            "d34e3b2957865fe76c73ec91d798f78de95f2b0e0cddfc47e341b5f235dc4d58"
        ));
        let res = parse_portal_log(&log, &fields, &Chain::Ethereum, Some(4638757), Some(1511886266), block_hash);
        assert_eq!(res.block_hash, block_hash);
        assert_eq!(res.removed, Some(false));
        assert_eq!(res.log_index, Some(5));
    }

    #[test]
    fn test_event_signature_filter_is_portal_supported() {
        assert!(filter_supported_by_portal(&LogFilter::EventSignature(
            "Transfer(address,address,uint256)".to_string()
        )));
        // block_hash filter is NOT Portal-serviceable.
        assert!(!filter_supported_by_portal(&LogFilter::BlockHash(alloy::primitives::B256::ZERO)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core resolve_logs::tests`
Expected: FAIL — `parse_portal_log` has the wrong arity (no `block_hash` param) and `EventSignature` is unsupported.

- [ ] **Step 3: Implement**

Extend the import (lines 1-3) to add `resolve_portal_bound`, `block_range_is_portal_eligible`, and `keccak256`:

```rust
use super::resolve_portal::{
    block_range_is_portal_eligible, portal_query, resolve_portal_bound, value_to_address,
    value_to_b256, value_to_bytes, value_to_u64,
};
use alloy::primitives::keccak256;
```

Add `EventSignature` to `filter_supported_by_portal` (43-53):

```rust
fn filter_supported_by_portal(filter: &LogFilter) -> bool {
    matches!(
        filter,
        LogFilter::BlockRange(_)
            | LogFilter::EmitterAddress(_)
            | LogFilter::EventSignature(_)
            | LogFilter::Topic0(_)
            | LogFilter::Topic1(_)
            | LogFilter::Topic2(_)
            | LogFilter::Topic3(_)
    )
}
```

Replace `extract_block_range` (56-72) with a version that finds the range filter and reports eligibility (resolution happens async in the Portal path):

```rust
/// Find the BlockRange filter, if present.
fn find_block_range(filters: &[LogFilter]) -> Option<&BlockRange> {
    filters.iter().find_map(|f| match f {
        LogFilter::BlockRange(range) => Some(range),
        _ => None,
    })
}
```

Add the import for `BlockRange` near the top: `use crate::common::block::BlockRange;` (adjust the existing `logs::{...}` / `chain::{...}` use lines — `BlockRange` currently is only referenced indirectly).

Rewrite the tail of `should_use_portal` (99-101) — replace the `extract_block_range(...).is_some()` check with an eligibility check:

```rust
    // Must have a Portal-resolvable block range.
    match find_block_range(logs.filter()) {
        Some(range) => block_range_is_portal_eligible(range),
        None => false,
    }
}
```

In `resolve_logs_via_portal`, replace `let (from_block, to_block) = extract_block_range(filters).unwrap();` (141) with:

```rust
    let range = find_block_range(filters).expect("should_use_portal guarantees a block range");
    let from_block = resolve_portal_bound(dataset, &range.start()).await?;
    let to_block = match range.end() {
        Some(end) => resolve_portal_bound(dataset, &end).await?,
        None => from_block,
    };
```

Map the `EventSignature` filter to `topic0` inside the filter-build loop (145-168) — add these arms:

```rust
            LogFilter::EventSignature(sig) => {
                let topic0 = keccak256(sig.as_bytes());
                log_filter.insert("topic0".into(), json!([format!("{:?}", topic0)]));
            }
            LogFilter::BlockHash(_) => {} // unreachable: gate excludes block_hash filter
```

Request the block hash from Portal when needed. After the `needs_block_timestamp` block (174-181) add:

```rust
    let needs_block_hash = fields.iter().any(|f| matches!(f, LogField::BlockHash));
    if needs_block_hash {
        block_fields.insert("hash".into(), json!(true));
    }
```

In the response loop (226-240), pull the block hash from the header and pass it down:

```rust
        let block_hash = header.and_then(|h| h.get("hash")).and_then(value_to_b256);
```

and change the `parse_portal_log(...)` call to pass `block_hash` as the final argument.

Update `parse_portal_log` (245-307): add a `block_hash: Option<B256>` parameter and replace the `_ => {}` arm so the match is exhaustive:

```rust
fn parse_portal_log(
    log: &serde_json::Value,
    fields: &[LogField],
    chain: &Chain,
    block_number: Option<u64>,
    block_timestamp: Option<u64>,
    block_hash: Option<alloy::primitives::B256>,
) -> LogQueryRes {
```

```rust
            LogField::BlockHash => {
                result.block_hash = block_hash;
            }
            LogField::Removed => {
                result.removed = Some(false);
            }
```

(Keep all existing arms; delete the `_ => {}`.)

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cargo test -p eql_core resolve_logs::tests`
Expected: PASS.

- [ ] **Step 5: Update the existing GET * logs e2e test for the Portal path**

`test_get_logs` now routes to Portal, which populates `block_timestamp` (RPC left it `None`). In `execution_engine.rs::test_get_logs`, change:

```rust
            block_number: Some(4638757),
            // TODO: the provider is returning None for block_timestamp
            block_timestamp: None,
```

to:

```rust
            block_number: Some(4638757),
            block_timestamp: Some(1511886266),
```

(Verified against Portal: block 4638757 timestamp = 1511886266; `block_hash`, `removed: Some(false)`, topics, data, tx hash/index, log_index all already match.)

- [ ] **Step 6: Run the e2e logs test**

Run: `cargo test -p eql_core test_get_logs`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/interpreter/backend/resolve_logs.rs crates/core/src/interpreter/backend/execution_engine.rs
git commit -m "feat(portal): serve all log fields + EventSignature filter via Portal"
```

---

## Task 6: Full-suite verification

**Files:** none (verification only).

- [ ] **Step 1: Build and run the whole crate test suite**

Run: `cargo test -p eql_core`
Expected: PASS. Pay attention to the previously-RPC tests that now route through Portal: `test_get_block_fields`, `test_get_logs`, `test_dump_results`, and the block case of `test_get_chain_field`.

- [ ] **Step 2: Clippy + fmt**

Run: `cargo clippy -p eql_core --all-targets && cargo fmt --check`
Expected: no warnings/diffs (fix any unused-import warnings from deleted functions).

- [ ] **Step 3: Sanity-check the fallback boundary is intact**

Confirm by reading, not just tests: `resolve_account.rs` unchanged (RPC), tx-by-hash still hits RPC (`should_use_portal` returns false when `ids().is_some()`), `block_hash` log filter still forces RPC (`filter_supported_by_portal` false), and `pending` tags still fall back (`tag_is_portal_eligible` false).

- [ ] **Step 4: Final commit (if any fixups)**

```bash
git add -A
git commit -m "chore(portal): clippy/fmt cleanup for Portal-primary routing"
```

---

## Self-Review notes (author)

- **Spec coverage:** §2 block/tx/log completion → Tasks 3/4/5; §3 tag resolution → Task 1 + wired in 3/4/5; §4 Fantom → Task 2 (superseded by the full Fantom removal — see Task 2 banner); §5 primitives → Task 1; §7 edge cases → covered (authorization_list None, block_hash filter → RPC, head-lag inherent). `account` explicitly untouched.
- **Known network dependence:** Tasks 3/4/5 Step-5/6 e2e tests hit live Ethereum/Portal, consistent with the existing suite. The unit tests (parse-back, mapping exhaustiveness, filter support) are deterministic and offline.
- **Drift guard:** every `match` over a field enum is exhaustive (no `_`), so a new enum variant fails compilation until handled — the structural fix for the original bug.
- **Verify-before-finalize:** Task 4 Step 5 includes a curl to confirm the chosen block/sender returns a row before locking the test constants.
