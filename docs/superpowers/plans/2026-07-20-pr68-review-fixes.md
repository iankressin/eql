# PR #68 Review Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the six accepted CodeRabbit findings from the PR #68 review: pagination-metadata integrity, HTTP timeouts on a shared Portal client, single-`/head`-snapshot range resolution, routing-decision unit tests, and two planning-doc alignment fixes.

**Architecture:** All code changes live in `crates/core/src/interpreter/backend/` (`resolve_portal.rs` and `resolve_transaction.rs` tests). No public API or routing behavior changes — only error-on-malformed-data, timeout hardening, one-snapshot `latest` resolution, and test coverage. Two documentation files get superseded/alignment annotations.

**Tech Stack:** Rust (Cargo workspace, MSRV 1.79 — `std::sync::OnceLock` is available, `std::sync::LazyLock` is NOT), `reqwest 0.12`, `serde_json`, `anyhow`, `alloy 0.6.4`. Tests are in-module `#[cfg(test)]` with the existing `test_support::spawn_mock_portal` mock server.

**Review source:** All findings are from CodeRabbit's review of PR #68 (<https://github.com/iankressin/eql/pull/68>). Comment IDs used in Task 6: `3609023499` (pagination), `3609023503` (head snapshot), `3609023507` (plan-doc Fantom), `3609023508` (design-doc semantics). The timeout finding and the routing-test nitpick have no inline thread (they live only in the review body), so they get one top-level PR comment.

## Global Constraints

- **Work in the PR worktree:** `/Users/ianguimaraes/Projects/eql/eql/.worktrees/portal-primary-routing-sdd`, branch `portal-primary-routing-sdd` (PR #68's head). All paths below are relative to that worktree root. Run all commands from that directory.
- MSRV is `rust-version = "1.79"` (workspace `Cargo.toml`). Use `OnceLock` (stable 1.70), never `LazyLock` (stable 1.80).
- Run tests with `cargo test -p eql_core`. Some pre-existing e2e tests hit live Ethereum RPC/Portal — network flakiness in *unrelated* tests is not caused by this plan; rerun if needed. Every NEW test in this plan is offline/deterministic.
- Timeout values: connect `10s`, total per-request `60s` (generous because each Portal `/stream` page can be several MB; `/head` is trivially fast).
- Do NOT `cfg`-gate the reqwest timeout code for wasm32: `crates/wasm` is dormant (last touched 2024-08), predates the Portal code entirely, and no CI pipeline builds `wasm32-unknown-unknown`.
- Error-message strings are load-bearing: existing tests assert exact messages. Copy them verbatim from this plan.
- Commit after every task, from the worktree, on branch `portal-primary-routing-sdd`.

---

## Task 1: Reject pagination pages containing items without `header.number`

**Why:** `next_portal_page_start` computes the pagination high-water mark with `filter_map`, silently skipping page items that lack a usable `header.number` — but `portal_query_with_base_url` still appends those items to the results. An unparseable item representing a block above the valid maximum would be re-fetched on the next page, duplicating data. All three resolvers unconditionally request `block.number` (`resolve_block.rs` "Always request number", `resolve_logs.rs` "Pagination metadata is always requested", `resolve_transaction.rs` `"block": { "number": true }`), so a missing `header.number` is always a malformed response and failing fast is correct.

**Files:**
- Modify: `crates/core/src/interpreter/backend/resolve_portal.rs` (function `next_portal_page_start`, currently lines 11–38; tests module at end of file)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `next_portal_page_start(page: &[Value], current_from_block: u64, to_block: u64) -> Result<Option<u64>>` — same signature, stricter contract: errors if ANY page item lacks a usable `header.number`. Two exact error messages later steps depend on:
  - per-item: `"Portal returned a page item without a usable header.number"`
  - empty-page (defensive, caller already skips empty pages): `"Portal returned a nonempty page without a usable header.number"` (unchanged)

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block of `crates/core/src/interpreter/backend/resolve_portal.rs` (next to the existing `test_next_portal_page_start_*` tests):

```rust
    #[test]
    fn test_next_portal_page_start_rejects_page_with_item_missing_header_number() {
        // A malformed item is appended to results but invisible to pagination:
        // advancing from the remaining maximum could re-fetch (duplicate) its data.
        let page = vec![
            json!({ "header": { "number": "0x2a" } }),
            json!({ "transactions": [{ "hash": "0x01" }] }),
        ];

        let error = next_portal_page_start(&page, 42, 50)
            .expect_err("a page item without header.number must fail pagination");

        assert_eq!(
            error.to_string(),
            "Portal returned a page item without a usable header.number"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p eql_core resolve_portal::tests::test_next_portal_page_start_rejects_page_with_item_missing_header_number`
Expected: FAIL — the current `filter_map` implementation returns `Ok(Some(43))`, so `expect_err` panics.

- [ ] **Step 3: Implement per-item validation**

In `crates/core/src/interpreter/backend/resolve_portal.rs`, replace the body of `next_portal_page_start` — specifically this existing code:

```rust
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
```

with:

```rust
    let mut last_block: Option<u64> = None;
    for block in page {
        let number = block
            .get("header")
            .and_then(|header| header.get("number"))
            .and_then(value_to_u64)
            .ok_or_else(|| {
                anyhow::anyhow!("Portal returned a page item without a usable header.number")
            })?;
        last_block = Some(last_block.map_or(number, |max| max.max(number)));
    }

    let last_block = last_block.ok_or_else(|| {
        anyhow::anyhow!("Portal returned a nonempty page without a usable header.number")
    })?;
```

The rest of the function (`if last_block < current_from_block { ... }` and the final `Ok(...)`) is unchanged.

- [ ] **Step 4: Run the module tests — expect exactly one legacy failure**

Run: `cargo test -p eql_core resolve_portal`
Expected: `test_next_portal_page_start_rejects_page_with_item_missing_header_number` now PASSES; `test_nonempty_page_without_header_number_is_an_error` now FAILS, because its page (a single item with no header at all) now hits the per-item error message first. All other tests pass.

- [ ] **Step 5: Update the legacy test's expected message**

In the same file, in `test_nonempty_page_without_header_number_is_an_error`, change:

```rust
        assert_eq!(
            error.to_string(),
            "Portal returned a nonempty page without a usable header.number"
        );
```

to:

```rust
        assert_eq!(
            error.to_string(),
            "Portal returned a page item without a usable header.number"
        );
```

- [ ] **Step 6: Run the module tests to verify all pass**

Run: `cargo test -p eql_core resolve_portal`
Expected: PASS (all tests in the module).

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/interpreter/backend/resolve_portal.rs
git commit -m "fix(portal): reject pagination pages containing items without header.number"
```

---

## Task 2: Share one Portal HTTP client with explicit timeouts

**Why:** `portal_query_with_base_url` and `portal_head` each call `reqwest::Client::new()` per invocation. reqwest 0.12 applies NO request timeout by default, so a stalled Portal endpoint hangs an EQL query forever; and a fresh `Client` per call discards the connection pool.

**Files:**
- Modify: `crates/core/src/interpreter/backend/resolve_portal.rs` (imports; new `portal_client()`; two call sites; tests module)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `fn portal_client() -> &'static reqwest::Client` (private to the module) — Task 3's code blocks call it. Constants `PORTAL_CONNECT_TIMEOUT: Duration = 10s`, `PORTAL_REQUEST_TIMEOUT: Duration = 60s`.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block of `resolve_portal.rs`:

```rust
    #[test]
    fn test_portal_client_is_shared_across_calls() {
        let first = portal_client() as *const reqwest::Client;
        let second = portal_client() as *const reqwest::Client;
        assert_eq!(first, second, "Portal requests must reuse one shared client");
    }
```

Note: the timeout values themselves are not inspectable through reqwest's public API; this test pins the sharing property, and the existing mock-server tests guard against request regressions.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p eql_core resolve_portal::tests::test_portal_client_is_shared_across_calls`
Expected: FAIL to COMPILE — `portal_client` is not defined. (A compile error in the test is the red step here.)

- [ ] **Step 3: Implement the shared client**

In `resolve_portal.rs`, change the imports at the top of the file from:

```rust
use std::str::FromStr;
```

to:

```rust
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Duration;
```

Directly below `const PORTAL_BASE_URL: &str = "https://portal.sqd.dev/datasets";` add:

```rust
const PORTAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PORTAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Shared HTTP client for all Portal traffic: one connection pool, and explicit
/// timeouts so a stalled Portal endpoint cannot hang a query indefinitely.
fn portal_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(PORTAL_CONNECT_TIMEOUT)
            .timeout(PORTAL_REQUEST_TIMEOUT)
            .build()
            .expect("failed to build shared Portal HTTP client")
    })
}
```

In `portal_query_with_base_url`, replace:

```rust
    let url = format!("{}/{}/stream", base_url, dataset);
    let client = reqwest::Client::new();
```

with:

```rust
    let url = format!("{}/{}/stream", base_url, dataset);
    let client = portal_client();
```

In `portal_head`, replace:

```rust
    let url = format!("{}/{}/head", PORTAL_BASE_URL, dataset);
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
```

with:

```rust
    let url = format!("{}/{}/head", PORTAL_BASE_URL, dataset);
    let response = portal_client()
        .get(&url)
```

- [ ] **Step 4: Run the module tests to verify they pass**

Run: `cargo test -p eql_core resolve_portal`
Expected: PASS — including the new sharing test and every existing mock-server test (the mock sends `Connection: close`, so pooling does not affect them).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/interpreter/backend/resolve_portal.rs
git commit -m "fix(portal): share one Portal HTTP client with explicit timeouts"
```

---

## Task 3: Resolve `latest` range bounds from a single `/head` snapshot

**Why:** `resolve_portal_range` resolves start and end independently, so `latest:latest` issues two `/head` requests. If the head advances between them, a single-block query silently widens to two blocks. Fetching the head once per range and resolving both bounds against that snapshot fixes the race and halves `/head` traffic.

**Files:**
- Modify: `crates/core/src/interpreter/backend/resolve_portal.rs` (the `portal_head` → `resolve_block_id_range` section; `test_support::read_json_request`; tests module)

**Interfaces:**
- Consumes: `portal_client()` from Task 2.
- Produces:
  - `pub async fn portal_head(dataset: &str) -> Result<u64>` (unchanged signature, now delegates)
  - `pub(crate) async fn portal_head_with_base_url(base_url: &str, dataset: &str) -> Result<u64>`
  - `fn resolve_bound_with_head(tag: &BlockNumberOrTag, head: Option<u64>) -> Result<u64>` (sync, private)
  - `pub async fn resolve_portal_range(dataset: &str, range: &BlockRange) -> Result<(u64, u64)>` (unchanged signature, now delegates)
  - `pub(crate) async fn resolve_portal_range_with_base_url(base_url: &str, dataset: &str, range: &BlockRange) -> Result<(u64, u64)>`
  - `pub async fn resolve_portal_bound(dataset: &str, tag: &BlockNumberOrTag) -> Result<u64>` and `pub async fn resolve_block_id_range(dataset: &str, id: &BlockId) -> Result<(u64, u64)>` keep their exact signatures and behavior. Callers in `resolve_block.rs`, `resolve_logs.rs`, `resolve_transaction.rs` need NO changes.

- [ ] **Step 1: Teach the mock server to record GET requests (no body)**

The new tests exercise `/head`, which is a GET with no `Content-Length`; `read_json_request` currently panics on that. In `test_support`, in `read_json_request`, replace:

```rust
        let mut body = vec![0; content_length.expect("request Content-Length")];
        reader.read_exact(&mut body).expect("read request body");
        serde_json::from_slice(&body).expect("parse request JSON")
```

with:

```rust
        let body_len = content_length.unwrap_or(0);
        if body_len == 0 {
            // GET requests (e.g. /head) carry no body; record them as null.
            return Value::Null;
        }
        let mut body = vec![0; body_len];
        reader.read_exact(&mut body).expect("read request body");
        serde_json::from_slice(&body).expect("parse request JSON")
```

Run: `cargo test -p eql_core resolve_portal`
Expected: PASS (pure widening of test infrastructure; POST requests still parse exactly as before).

- [ ] **Step 2: Write the failing tests**

Add to the `mod tests` block of `resolve_portal.rs`:

```rust
    #[tokio::test]
    async fn test_latest_latest_range_resolves_both_bounds_from_one_head_snapshot() {
        // Two canned /head responses with DIFFERENT numbers: a correct
        // implementation fetches the head once, so both bounds resolve to 100.
        // A per-bound implementation would observe 100 then 101 and widen the
        // single-block range. The handle is deliberately not joined: the second
        // canned response must go unused.
        let (base_url, requests, _handle) = test_support::spawn_mock_portal(vec![
            "{\"number\":100}".to_string(),
            "{\"number\":101}".to_string(),
        ]);
        let range = BlockRange::new(BlockNumberOrTag::Latest, Some(BlockNumberOrTag::Latest));

        let resolved = resolve_portal_range_with_base_url(&base_url, "test", &range)
            .await
            .expect("latest:latest must resolve via a single head snapshot");

        assert_eq!(resolved, (100, 100));
        assert_eq!(
            requests.lock().expect("captured requests").len(),
            1,
            "resolving latest:latest must hit /head exactly once"
        );
    }

    #[tokio::test]
    async fn test_concrete_range_never_calls_head() {
        // 127.0.0.1:9 (discard port) refuses connections — this only passes if
        // no /head request is attempted for earliest/number bounds.
        let range = BlockRange::new(BlockNumberOrTag::Earliest, Some(BlockNumberOrTag::Number(5)));

        let resolved = resolve_portal_range_with_base_url("http://127.0.0.1:9", "unused", &range)
            .await
            .expect("concrete bounds must resolve without any network call");

        assert_eq!(resolved, (0, 5));
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p eql_core resolve_portal`
Expected: FAIL to COMPILE — `resolve_portal_range_with_base_url` is not defined.

- [ ] **Step 4: Implement the single-snapshot resolution**

In `resolve_portal.rs`, replace the entire section from the `/// Fetch the current head block number...` doc comment through the closing brace of `resolve_block_id_range` (as of the end of Task 2 it contains `portal_head`, `resolve_portal_bound`, `resolve_portal_range`, `resolve_block_id_range`) with:

```rust
/// Fetch the current head block number for a dataset from Portal's `/head` endpoint.
pub async fn portal_head(dataset: &str) -> Result<u64> {
    portal_head_with_base_url(PORTAL_BASE_URL, dataset).await
}

pub(crate) async fn portal_head_with_base_url(base_url: &str, dataset: &str) -> Result<u64> {
    let url = format!("{}/{}/head", base_url, dataset);
    let response = portal_client()
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

/// Resolve a single block tag against an optional pre-fetched head snapshot.
fn resolve_bound_with_head(tag: &BlockNumberOrTag, head: Option<u64>) -> Result<u64> {
    match tag {
        BlockNumberOrTag::Number(n) => Ok(*n),
        BlockNumberOrTag::Earliest => Ok(0),
        BlockNumberOrTag::Latest => head.ok_or_else(|| {
            anyhow::anyhow!("Portal head snapshot missing while resolving 'latest'")
        }),
        other => Err(anyhow::anyhow!(
            "Block tag {:?} cannot be resolved via Portal",
            other
        )),
    }
}

/// Resolve a single block tag to a concrete number using Portal.
pub async fn resolve_portal_bound(dataset: &str, tag: &BlockNumberOrTag) -> Result<u64> {
    let head = match tag {
        BlockNumberOrTag::Latest => Some(portal_head(dataset).await?),
        _ => None,
    };
    resolve_bound_with_head(tag, head)
}

/// Resolve and validate a Portal block range after tags become concrete numbers.
/// `latest` bounds are resolved against a single `/head` snapshot, so a range
/// like `latest:latest` cannot straddle two consecutive heads.
pub async fn resolve_portal_range(dataset: &str, range: &BlockRange) -> Result<(u64, u64)> {
    resolve_portal_range_with_base_url(PORTAL_BASE_URL, dataset, range).await
}

pub(crate) async fn resolve_portal_range_with_base_url(
    base_url: &str,
    dataset: &str,
    range: &BlockRange,
) -> Result<(u64, u64)> {
    let start_tag = range.start();
    let end_tag = range.end();

    let needs_head = matches!(start_tag, BlockNumberOrTag::Latest)
        || matches!(end_tag, Some(BlockNumberOrTag::Latest));
    let head = if needs_head {
        Some(portal_head_with_base_url(base_url, dataset).await?)
    } else {
        None
    };

    let start = resolve_bound_with_head(&start_tag, head)?;
    let end = match end_tag {
        Some(end) => resolve_bound_with_head(&end, head)?,
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
```

- [ ] **Step 5: Run the module tests to verify they pass**

Run: `cargo test -p eql_core resolve_portal`
Expected: PASS — both new tests, plus the existing `test_resolve_block_id_range_rejects_reversed_tagged_range` and `test_resolve_block_id_range_omitted_end_is_one_resolved_block` (neither uses `latest`, so neither touches the network, same as before).

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p eql_core`
Expected: PASS (live-network e2e tests included; rerun on unrelated network flakes).

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/interpreter/backend/resolve_portal.rs
git commit -m "fix(portal): resolve latest range bounds from a single head snapshot"
```

---

## Task 4: Pin transaction routing decisions with `should_use_portal` unit tests

**Why:** The Portal e2e tests in `execution_engine.rs` (`test_get_transactions_via_portal_block_range`, `test_get_transactions_via_portal_authorization_list_only_retains_rows`) assert result shape only — block 20000000 is pre-Pectra, so an RPC fallback would produce identical assertions (every `authorization_list` is `None`, `hash`/`from` populated). No unit test currently covers `should_use_portal` in `resolve_transaction.rs`. Offline unit tests pin the routing contract directly; this is the "focused should_use_portal test" option CodeRabbit offered, chosen over plumbing a mock-endpoint override through `ExecutionEngine` (disproportionate for a trivial-severity nit).

**Files:**
- Modify: `crates/core/src/interpreter/backend/resolve_transaction.rs` (tests module only; `should_use_portal` is in scope via `use super::*`)

**Interfaces:**
- Consumes: `should_use_portal(chain: &ChainOrRpc, transaction: &Transaction) -> bool` (existing, private to the module); `Transaction::new(ids: Option<Vec<B256>>, filters: Option<Vec<TransactionFilter>>, fields: Vec<TransactionField>)`. `TransactionFilter` is NOT `Clone` — construct filters fresh via a closure.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block of `resolve_transaction.rs`:

```rust
    #[test]
    fn test_should_use_portal_accepts_the_e2e_block_range_shape() {
        // Pins the routing decision the execution_engine Portal e2e tests rely
        // on: their result-shape assertions alone would also pass via RPC.
        let transaction = Transaction::new(
            None,
            Some(vec![TransactionFilter::BlockId(BlockId::Range(
                BlockRange::new(
                    BlockNumberOrTag::Number(20_000_000),
                    Some(BlockNumberOrTag::Number(20_000_000)),
                ),
            ))]),
            vec![TransactionField::Hash],
        );

        assert!(should_use_portal(
            &ChainOrRpc::Chain(Chain::Ethereum),
            &transaction
        ));
    }

    #[test]
    fn test_should_use_portal_rejects_rpc_only_shapes() {
        let range_filter = || {
            TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                BlockNumberOrTag::Number(1),
                Some(BlockNumberOrTag::Number(2)),
            )))
        };
        let ethereum = ChainOrRpc::Chain(Chain::Ethereum);

        // By-hash queries: Portal has no hash filter.
        let by_hash = Transaction::new(
            Some(vec![FixedBytes::<32>::ZERO]),
            Some(vec![range_filter()]),
            vec![TransactionField::Hash],
        );
        assert!(!should_use_portal(&ethereum, &by_hash));

        // No block filter: Portal needs a range to scan.
        let no_block_filter = Transaction::new(None, None, vec![TransactionField::Hash]);
        assert!(!should_use_portal(&ethereum, &no_block_filter));

        // A pending bound is not Portal-resolvable.
        let pending = Transaction::new(
            None,
            Some(vec![TransactionFilter::BlockId(BlockId::Range(
                BlockRange::new(
                    BlockNumberOrTag::Number(1),
                    Some(BlockNumberOrTag::Pending),
                ),
            ))]),
            vec![TransactionField::Hash],
        );
        assert!(!should_use_portal(&ethereum, &pending));

        // Explicit RPC URLs must never route to Portal.
        let eligible = Transaction::new(
            None,
            Some(vec![range_filter()]),
            vec![TransactionField::Hash],
        );
        assert!(!should_use_portal(
            &ChainOrRpc::Rpc("http://localhost:8545".parse().unwrap()),
            &eligible
        ));
    }
```

- [ ] **Step 2: Run tests to verify they run (and pass)**

Run: `cargo test -p eql_core resolve_transaction::tests::test_should_use_portal`
Expected: PASS — these tests pin EXISTING behavior (characterization tests), so green-on-first-run is the expected outcome here, not a TDD red. To verify each assertion actually exercises the gate, temporarily invert any one assertion (e.g. change `assert!(should_use_portal(...))` to `assert!(!should_use_portal(...))` in the first test), run again, watch it FAIL, then revert the inversion.

- [ ] **Step 3: Run the module tests**

Run: `cargo test -p eql_core resolve_transaction`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/interpreter/backend/resolve_transaction.rs
git commit -m "test(portal): pin transaction routing decisions with should_use_portal unit tests"
```

---

## Task 5: Align the planning docs with the shipped implementation

**Why:** Two committed planning documents contradict the code this PR ships. (a) The Portal-routing plan still contains a "Fantom → RPC" task, but the same PR removes `Chain::Fantom` entirely — an agent re-executing that task would target code that no longer exists. (b) The design spec claims `value_to_status_bool` cannot parse hex strings (it now can, with a test) and that authorization-list-only Portal queries drop null-only rows (they are retained, with two tests). Annotate — do not rewrite history.

**Files:**
- Modify: `docs/superpowers/plans/2026-07-17-portal-primary-routing.md` (lines 7, 19, the Task 2 heading at ~207, the self-review bullet at ~927)
- Modify: `docs/superpowers/specs/2026-07-17-portal-primary-routing-design.md` (the §5 implementation note at ~94, the §7 authorization-list bullet at ~140)

**Interfaces:**
- Consumes / Produces: nothing — documentation only.

- [ ] **Step 1: Mark the plan's Fantom material as superseded**

In `docs/superpowers/plans/2026-07-17-portal-primary-routing.md`, make these four edits.

Edit 1 — in the `**Architecture:**` line (line 7), replace the final sentence:

```
Fix Fantom's dead dataset mapping.
```

with:

```
Fix Fantom's dead dataset mapping. *(Superseded: Fantom was later removed as a chain entirely — see `2026-07-17-remove-fantom-support.md`.)*
```

Edit 2 — in the Global Constraints bullet (line 19), replace:

```
and chains without a Portal dataset (`ronin`/`kava`/`mekong`/`fantom`) all stay on RPC.
```

with:

```
and chains without a Portal dataset (`ronin`/`kava`/`mekong`) all stay on RPC. *(`fantom` was originally in this list; it was later removed as a chain entirely — see `2026-07-17-remove-fantom-support.md`.)*
```

Edit 3 — directly below the heading `## Task 2: Fantom → RPC (fix dead Portal dataset)` insert:

```
> **SUPERSEDED — do not execute.** This task kept `Chain::Fantom` and routed it to RPC. The product decision changed to removing Fantom entirely, and `Chain::Fantom` no longer exists. See `docs/superpowers/plans/2026-07-17-remove-fantom-support.md` and `docs/superpowers/specs/2026-07-17-remove-fantom-support-design.md`.
```

Edit 4 — in the Self-Review notes (line ~927), replace:

```
§4 Fantom → Task 2;
```

with:

```
§4 Fantom → Task 2 (superseded by the full Fantom removal — see Task 2 banner);
```

- [ ] **Step 2: Align the design spec's decoder and null-row statements**

In `docs/superpowers/specs/2026-07-17-portal-primary-routing-design.md`, make these two edits.

Edit 1 — in the §5 implementation note (line ~94), replace:

```
**Do not reuse `value_to_status_bool` blindly** — it only handles JSON bool / integer, not hex strings, and Portal serializes numeric fields as hex strings (`"0x1"`). The plan must use/add a decoder that maps both integer `0/1` and hex `0x0`/`0x1` → `bool` (i.e. parse via `value_to_u64` first, then `!= 0`).
```

with:

```
**As implemented:** parity decoding uses the dedicated `value_to_parity_bool` (bool, int, or hex; maps `0/1`, legacy `27/28`, and EIP-155 `≥35` values to a parity bit), and `value_to_status_bool` itself was extended to decode hex strings as well as JSON bools/integers.
```

Edit 2 — in §7 (line ~140), replace the bullet:

```
- `authorization_list`-only query (`GET authorization_list FROM transaction WHERE block N`): every field is `None` from Portal, so `TransactionQueryRes::has_value()` is false and rows are dropped → empty result. Since the value is `null` anyway this is acceptable; note it in the plan (and it does not affect `GET *`, which always requests `hash`).
```

with:

```
- `authorization_list`-only query (`GET authorization_list FROM transaction WHERE block N`): Portal serves the field as `null`, and — as implemented — such null-only rows are **retained** (one row per matching transaction, `authorization_list: null`), matching the RPC path for legacy transactions. (`GET *` is unaffected; it always requests `hash`.)
```

- [ ] **Step 3: Verify the annotations landed and nothing else changed**

Run: `git diff --stat`
Expected: exactly two files changed — `docs/superpowers/plans/2026-07-17-portal-primary-routing.md` and `docs/superpowers/specs/2026-07-17-portal-primary-routing-design.md`.

Run: `grep -c "SUPERSEDED" docs/superpowers/plans/2026-07-17-portal-primary-routing.md`
Expected: `1`

Run: `grep -c "As implemented" docs/superpowers/specs/2026-07-17-portal-primary-routing-design.md`
Expected: `1` or more.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/plans/2026-07-17-portal-primary-routing.md docs/superpowers/specs/2026-07-17-portal-primary-routing-design.md
git commit -m "docs: align Portal routing plan/spec with shipped implementation"
```

---

## Task 6: Final validation, push, and review-thread replies

**Why:** Verify the whole branch, publish the fixes to PR #68, and close the loop on each review thread (replies go in the inline comment threads, not as drive-by top-level comments — except the two findings that have no inline thread).

**Files:**
- Create: `docs/superpowers/plans/2026-07-20-pr68-review-fixes.md` is committed here (this plan document — it already exists on disk in the worktree).

**Interfaces:**
- Consumes: all commits from Tasks 1–5.

- [ ] **Step 1: Run the full validation suite**

```bash
cargo build
cargo test -p eql_core
git diff --check
```

Expected: build succeeds; all tests pass (rerun once on unrelated live-network flakes); no whitespace errors. Do not proceed to push unless all three are clean.

- [ ] **Step 2: Commit this plan document**

```bash
git add docs/superpowers/plans/2026-07-20-pr68-review-fixes.md
git commit -m "docs: add PR #68 review-fix plan"
```

- [ ] **Step 3: Push the branch**

```bash
git push origin portal-primary-routing-sdd
```

Expected: PR #68 updates with the new commits.

- [ ] **Step 4: Reply in each inline review thread**

```bash
SHA=$(git rev-parse --short HEAD)

gh api repos/iankressin/eql/pulls/68/comments/3609023499/replies \
  -f body="Fixed: next_portal_page_start now errors on any page item lacking a usable header.number instead of skipping it, so malformed items can no longer be appended while being invisible to pagination. Added a mixed valid/invalid page regression test. (branch head: $SHA)"

gh api repos/iankressin/eql/pulls/68/comments/3609023503/replies \
  -f body="Fixed: resolve_portal_range now fetches /head at most once per range and resolves both bounds against that single snapshot, so latest:latest can no longer straddle two consecutive heads. Covered by a mock-Portal test asserting exactly one /head request. (branch head: $SHA)"

gh api repos/iankressin/eql/pulls/68/comments/3609023507/replies \
  -f body="Fixed: Task 2 of the plan now carries a SUPERSEDED banner linking to the Fantom-removal plan/design, and the remaining Fantom references (architecture line, RPC-fallback constraint, self-review note) are annotated to match the full removal. (branch head: $SHA)"

gh api repos/iankressin/eql/pulls/68/comments/3609023508/replies \
  -f body="Fixed: the design spec now describes value_to_status_bool as hex-aware and documents that authorization-list-only Portal queries retain null-only rows, matching the shipped contracts. (branch head: $SHA)"
```

Expected: each call returns the created reply JSON.

- [ ] **Step 5: Answer the two findings that have no inline thread**

The timeout finding (outside diff range) and the routing-assertion nitpick exist only in the review body, so reply once at PR level:

```bash
SHA=$(git rev-parse --short HEAD)

gh pr comment 68 --repo iankressin/eql --body "Addressed the two review-body findings as well (branch head: $SHA):

- **Portal client timeouts (outside-diff, Major):** all Portal traffic now goes through one shared reqwest client built with connect_timeout=10s and timeout=60s (OnceLock; LazyLock is unavailable at the workspace MSRV of 1.79), so a stalled Portal endpoint can no longer hang a query indefinitely.
- **Portal routing test assertion (nitpick):** added offline unit tests for should_use_portal in resolve_transaction.rs that pin the exact e2e query shape to Portal routing, plus negative cases (by-hash, no block filter, pending bound, explicit RPC URL). Chose the focused-unit-test option over injecting a mock Portal endpoint through ExecutionEngine, which would have required a public API seam disproportionate to the nit."
```

Expected: comment appears on PR #68.

---

## Self-Review (author)

- **Finding coverage:** pagination integrity → Task 1; timeouts/shared client → Task 2; single head snapshot → Task 3; routing-assertion nitpick → Task 4; plan-doc Fantom contradiction + design-doc stale semantics → Task 5; every finding gets a thread reply → Task 6. All six accepted findings have a task; none were rejected.
- **Placeholder scan:** every code step contains the complete code; every command has expected output. The only runtime-computed value is `$SHA` in Task 6, produced by a command included in the step.
- **Type consistency:** `portal_client()` is defined in Task 2 and consumed verbatim in Task 3's code block. `resolve_portal_range_with_base_url` / `portal_head_with_base_url` names match between Task 3's implementation and its tests. Error strings in Task 1's implementation match its tests character-for-character. Task 4 uses only APIs verified to exist (`Transaction::new(Option<Vec<B256>>, Option<Vec<TransactionFilter>>, Vec<TransactionField>)`, `FixedBytes::<32>::ZERO`, `ChainOrRpc::Rpc(Url)` via `.parse().unwrap()` — the same pattern as `parser.rs`).
- **Behavioral invariants:** no public signature changes; `resolve_block.rs` / `resolve_logs.rs` / `resolve_transaction.rs` call sites untouched by Tasks 1–3. Existing exact-message tests updated only where the message intentionally changed (Task 1 Step 5).
