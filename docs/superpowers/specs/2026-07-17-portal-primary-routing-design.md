# Route block, transaction, and log queries entirely through SQD Portal

**Date:** 2026-07-17
**Status:** Approved (design)
**Scope:** `crates/core` — Portal/RPC routing for `block`, `transaction`, `log` entities

---

## 1. Background & problem

Commit `9c8a311` introduced Portal-primary routing for block/log/transaction queries, with RPC as a fallback selected by a per-resolver `should_use_portal()` gate. Each resolver hardcodes its own `field_supported_by_portal()` allowlist:

- `resolve_block.rs:30-43`
- `resolve_transaction.rs:35-52`
- `resolve_logs.rs:23-39`

These allowlists have **drifted behind the field enums**. Any query selecting a field missing from an allowlist falls back to RPC — most importantly `GET *`, which expands to *every* enum variant (`block.rs:86-90`, `transaction.rs:152-156`, `logs.rs:93-97`) via the `EnumVariants` derive. So `GET * FROM block|log|transaction` **always** falls back to RPC today, even though Portal serves virtually all of the data.

Cross-referencing every EQL field against Portal's EVM OpenAPI schema (`https://beta.docs.sqd.dev/en/api/catalog/evm/openapi.yaml`) confirms: nearly every field the allowlists treat as "RPC-only" is in fact available on Portal. The allowlists are simply incomplete.

## 2. Goal & non-goals

**Goal:** Make Portal the primary path for block/log/transaction queries **including `GET *`**. RPC remains only for cases Portal *structurally* cannot serve.

**Non-goals:**
- Serving the `account` entity (balance/nonce/code) via Portal — impossible; Portal has no current-state reads, only per-block state diffs. `resolve_account.rs` stays 100% RPC.
- Transaction-by-hash via Portal — impossible; Portal filters transactions only by `from`/`to`/`sighash`, there is no `hash` filter.
- Adding a `Sonic` chain (possible future follow-up).
- Runtime RPC failover when a Portal request errors — out of scope; keep today's behavior (Portal errors propagate).

## 3. Locked decisions

1. **`authorization_list` (EIP-7702):** served via Portal as `null`. Portal has no `authorizationList` field. Only type-4 transactions lose it (`null` is already correct for every other tx). The transaction-by-hash path (RPC) still fills it correctly. Documented gap.
2. **Block tags:** `latest` → Portal `/head`; `earliest` → `0`; `pending` (and `finalized`/`safe`) → RPC (Portal does not index unconfirmed blocks; head-vs-finalized semantics kept on RPC conservatively).
3. **Fantom → RPC.** `portal_dataset()` returns `None` for `Fantom`. `fantom-mainnet` 404s (dataset dropped); `sonic-mainnet` is a *different* chain (id 146) and must not be substituted.
4. **Remove the field-based routing gate** for block/log/tx. Once mappings are complete, every field is Portal-serviceable (some via correct defaults), so the field check is always-true. The **field→Portal-name mapping becomes the single source of truth**; routing is decided by *query shape* only.
5. **Log `Removed` → `Some(false)`** (correct for finalized/indexed data). **Log `BlockHash` → from the joined block header.** **`EventSignature` filter → `topic0 = keccak256(signature)`.**

## 4. Routing model (after change)

Field coverage no longer influences routing for block/log/tx. Routing depends only on query shape:

| Entity | Uses Portal when… | Falls back to RPC when… |
|---|---|---|
| **Block** | all ids are `Number` / `latest` / `earliest`, and chain has a dataset | any id is `pending`/`finalized`/`safe`; no dataset |
| **Log** | resolvable block range; every filter ∈ {`BlockRange`, `EmitterAddress`, `Topic0-3`, `EventSignature`} | a `BlockHash` filter is present; range end is `pending`; no dataset |
| **Transaction** | not by-hash (`ids()` is `None`), has a block filter, range resolvable (`Number`/`latest`/`earliest`) | `transaction.ids()` is set (by hash); no block filter; `pending`; no dataset |

`account` is unconditionally RPC (no Portal path exists).

## 5. Field-mapping reference (complete)

### Block — `BlockField` → Portal name → decoder

| EQL field | Portal name | `BlockQueryRes` type | Decoder |
|---|---|---|---|
| Number | number | `u64` | `value_to_u64` |
| Timestamp | timestamp | `u64` | `value_to_u64` |
| Hash | hash | `B256` | `value_to_b256` |
| ParentHash | parentHash | `B256` | `value_to_b256` |
| StateRoot | stateRoot | `B256` | `value_to_b256` |
| TransactionsRoot | transactionsRoot | `B256` | `value_to_b256` |
| ReceiptsRoot | receiptsRoot | `B256` | `value_to_b256` |
| BaseFeePerGas | baseFeePerGas | `u64` | `value_to_u64` |
| **Size** | size | `U256` | `value_to_u256` |
| **LogsBloom** | logsBloom | `Bloom` | **`value_to_bloom` (new)** |
| **ExtraData** | extraData | `Bytes` | `value_to_bytes` |
| **MixHash** | mixHash | `B256` | `value_to_b256` |
| **TotalDifficulty** | totalDifficulty | `U256` | `value_to_u256` |
| **WithdrawalsRoot** | withdrawalsRoot | `B256` | `value_to_b256` |
| **BlobGasUsed** | blobGasUsed | `u64` | `value_to_u64` |
| **ExcessBlobGas** | excessBlobGas | `u64` | `value_to_u64` |
| **ParentBeaconBlockRoot** | parentBeaconBlockRoot | `B256` | `value_to_b256` |
| Chain | — | `Chain` | (set locally) |

Bold = newly added. After this, `parse_portal_block_header` handles every variant (drop the `_ => {}` arm).

### Transaction — `TransactionField` → Portal name → decoder

Existing: Type, Hash, From, To, Data(`input`), Value, GasPrice, GasLimit(`gas`), Status, ChainId, MaxFeePerGas, MaxPriorityFeePerGas, Chain.

Newly added:

| EQL field | Portal name | `TransactionQueryRes` type | Decoder |
|---|---|---|---|
| EffectiveGasPrice | effectiveGasPrice | `u128` | `value_to_u128` |
| V | v | `bool` | parity decode — see note |
| R | r | `U256` | `value_to_u256` |
| S | s | `U256` | `value_to_u256` |
| MaxFeePerBlobGas | maxFeePerBlobGas | `u128` | `value_to_u128` |
| YParity | yParity | `bool` | parity decode — see note |
| **AuthorizationList** | *(none)* | `Option<Vec<SignedAuthorization>>` | left `None` |

Implementation note: `v` and `y_parity` are both `Option<bool>` in EQL (parity semantics — the RPC path derives both from `signature().v()`). Decode both from Portal's parity value (`v`/`yParity`). **Do not reuse `value_to_status_bool` blindly** — it only handles JSON bool / integer, not hex strings, and Portal serializes numeric fields as hex strings (`"0x1"`). The plan must use/add a decoder that maps both integer `0/1` and hex `0x0`/`0x1` → `bool` (i.e. parse via `value_to_u64` first, then `!= 0`). Legacy-tx `v` values like `27/28` are an edge case; verify the actual Portal representation against a live type-0 tx and cover with a test. Confirm empirically whether Portal emits these as hex strings or JSON integers before finalizing the decoder.

### Log — `LogField` handling

Existing Portal-mapped: Address, Topic0-3(`topics` array, indexed access), Data, BlockNumber (from header), BlockTimestamp (from header), TransactionHash, TransactionIndex, LogIndex, Chain.

Newly handled:

| EQL field | Source | Result |
|---|---|---|
| **BlockHash** | request `block.hash`, carry from header (like `block_number`) | `value_to_b256(header.hash)` |
| **Removed** | not a Portal field | `Some(false)` |

Log filters — `filter_supported_by_portal` adds `EventSignature`, mapped to `topic0`:
- `LogFilter::EventSignature(sig)` → `topic0 = format!("{:?}", keccak256(sig.as_bytes()))`. Matches alloy's `Filter::event(sig)` used on the RPC path (`logs.rs:211`).
- `LogFilter::BlockHash(_)` remains **unsupported** → forces RPC (Portal has no block-by-hash lookup).
- Edge case: if both `EventSignature` and `Topic0` are present they both target `topic0`. Define behavior in the plan (recommend: explicit `Topic0` wins, or error) — this is an already-ambiguous query.

## 6. File-by-file changes

**`resolve_portal.rs`**
- Add `value_to_bloom(v: &Value) -> Option<Bloom>` (hex string → `Bloom::from_str`).
- Add `pub async fn portal_head(dataset: &str) -> Result<u64>` — `GET {PORTAL_BASE_URL}/{dataset}/head`, parse `.number`.

**`resolve_block.rs`**
- Extend `block_field_to_portal_name` (186-198) with the 9 new mappings.
- Extend `parse_portal_block_header` (201-243) to decode all variants; remove `_ => {}`.
- Remove `field_supported_by_portal` (30-43). Replace `block_id_is_concrete` (46-57) with `block_id_is_portal_eligible` (accepts `Number`/`Latest`/`Earliest`; rejects `Pending`/`Finalized`/`Safe`). Update `should_use_portal` (60-69) accordingly.
- In `resolve_blocks_via_portal` (114-164), resolve each bound to a concrete number before building `fromBlock`/`toBlock`: `Number(n)→n`, `Latest→portal_head(dataset)`, `Earliest→0`.

**`resolve_transaction.rs`**
- Extend `tx_field_to_portal_name` (224-240) with the 6 new mappings.
- Extend `parse_portal_transaction` (243-297) to decode the 6 new fields; add an explicit `AuthorizationList => {}` (stays `None`).
- Remove `field_supported_by_portal` (35-52) and its use in `should_use_portal` (130-133). Keep the shape gates: `ids().is_some() → RPC` (112), `has_block_filter` (116), range resolvable. Extend `block_id_to_concrete_range` (55-72) usage so `latest`/`earliest` resolve via `portal_head`/`0` in the Portal path (they currently return `None` → RPC).

**`resolve_logs.rs`**
- In `resolve_logs_via_portal` (129-243): when `BlockHash` requested, add `block.hash` to the block field selection and carry it from the header; set `removed = Some(false)` when `Removed` requested; map `EventSignature` filter → `topic0`.
- Extend `parse_portal_log` (245-307): `BlockHash` from header hash, `Removed → Some(false)`; remove the `_ => {}` for these.
- `filter_supported_by_portal` (43-53): add `EventSignature`. Keep `BlockHash` unsupported.
- Remove field gate from `should_use_portal` (75-101); keep filter-eligibility + resolvable-range checks. Extend `extract_block_range` (56-72) so a `latest`/`earliest` range bound is resolvable via Portal rather than returning `None`.

**`chain.rs`**
- `Chain::Fantom => None` in `portal_dataset()` (112). (Optional, out of scope: the pre-existing missing `Ronin`/`2020` arm in `TryFrom<u64>` at 246-276 — note only.)

## 7. Error handling & edge cases

- `portal_head` failure → propagate (`latest` unresolvable). No silent RPC failover.
- `authorization_list`-only query (`GET authorization_list FROM transaction WHERE block N`): every field is `None` from Portal, so `TransactionQueryRes::has_value()` is false and rows are dropped → empty result. Since the value is `null` anyway this is acceptable; note it in the plan (and it does not affect `GET *`, which always requests `hash`).
- Explicit block numbers beyond Portal head → Portal returns empty (not an error).
- Portal head can lag the chain tip → `latest` via Portal may resolve a few blocks behind RPC's `latest`. Documented behavior change.

## 8. Testing

- **Mapping completeness guard (unit):** a test asserting every `BlockField`/`TransactionField`/`LogField` variant is either mapped to a Portal name or explicitly defaulted — so the allowlist-drift bug cannot recur.
- **Decoder unit tests:** `value_to_bloom` round-trip; `v`/`y_parity` parity decoding; `size`/`total_difficulty` as `U256`.
- **Routing tests:** `GET *` for block/log/tx → Portal; tx-by-hash, `pending` tag, `block_hash` log filter, and `fantom`/`ronin`/`kava`/`mekong` → RPC; `latest`/`earliest` block/range → Portal.
- **Parity tests:** for a known block, transaction, and log, assert the Portal result equals the RPC result field-by-field, excluding the documented `authorization_list` (type-4) and `removed` caveats. This is the primary proof the migration is faithful.

## 9. Risks

- **`latest` semantics shift:** Portal head may trail RPC `latest`. Acceptable per decision; documented.
- **`authorization_list` null** for type-4 txs in range queries. Acceptable per decision; documented.
- **Parse-back correctness** across all new fields (esp. `v`/`y_parity` bool, `Bloom`). Mitigated by parity + decoder tests.
