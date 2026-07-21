# Remove Fantom Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Fantom from EQL's named chain model, selectors, chain-ID conversion, bundled configuration, and supported-network documentation.

**Architecture:** Delete `Chain::Fantom` so existing exhaustive matches force the Rust compiler to expose every remaining integration point. Remove the corresponding Pest selector and documentation entries, then test rejection at both public conversion boundaries. This plan supersedes Task 2 of `2026-07-17-portal-primary-routing.md`.

**Tech Stack:** Rust, Pest grammar, Markdown documentation, Cargo tests.

## Global Constraints

- `fantom` must be rejected as an invalid named chain.
- Chain ID `250` must be rejected as an invalid chain.
- `Chain::all_variants()` and wildcard selection must not include Fantom.
- Remove Fantom's default RPC and supported-network documentation.
- Keep generic custom-RPC support unchanged for recognized chain IDs; an RPC reporting chain ID `250` is intentionally unsupported.
- Do not substitute Sonic (chain ID `146`) or add a compatibility alias.
- Do not rewrite the existing interim RPC-fallback commit; land full removal as a follow-up commit.
- Production and test code changes belong in `crates/core`; run Rust tests with `cargo test -p eql_core`.

---

## Task 1: Remove Fantom from the supported-chain surface

**Files:**
- Modify: `crates/core/src/common/chain.rs`
- Modify: `crates/core/src/interpreter/frontend/productions.pest`
- Modify: `docs/installation.md`
- Test: `crates/core/src/common/chain.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: existing `Chain`, `ChainError`, `Chain::all_variants()`, `TryFrom<&str> for Chain`, and `TryFrom<u64> for Chain`.
- Produces: `fantom` and `250` return `ChainError::InvalidChain`; wildcard variants contain no Fantom entry.

- [ ] **Step 1: Replace the interim fallback test with failing removal tests**

Replace `test_fantom_has_no_portal_dataset` in `chain.rs` and retain the supported-chain assertion so the test module is:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fantom_name_is_unsupported() {
        let result = Chain::try_from("fantom");
        assert!(matches!(
            result,
            Err(ChainError::InvalidChain(ref value)) if value == "fantom"
        ));
    }

    #[test]
    fn test_fantom_chain_id_is_unsupported() {
        let result = Chain::try_from(250_u64);
        assert!(matches!(
            result,
            Err(ChainError::InvalidChain(ref value)) if value == "250"
        ));
    }

    #[test]
    fn test_wildcard_variants_exclude_fantom() {
        assert!(Chain::all_variants()
            .iter()
            .all(|chain| chain.to_string() != "fantom"));
    }

    #[test]
    fn test_supported_chain_still_has_dataset() {
        assert_eq!(Chain::Ethereum.portal_dataset(), Some("ethereum-mainnet"));
    }
}
```

- [ ] **Step 2: Run the focused tests to verify RED**

Run:

```bash
cargo test -p eql_core common::chain::tests -- --nocapture
```

Expected: FAIL. The name, ID, and wildcard tests observe the still-present `Chain::Fantom`; the supported Ethereum assertion passes.

- [ ] **Step 3: Delete Fantom from the Rust chain model**

In `chain.rs`, delete the enum variant:

```diff
     Ronin,
-    Fantom,
     Kava,
```

Delete these exact exhaustive-match/conversion arms:

```diff
-            Chain::Fantom => None,
-            Chain::Fantom => "https://fantom.drpc.org",
-            "fantom" => Ok(Chain::Fantom),
-            Chain::Fantom => 250,
-            250 => Ok(Chain::Fantom),
-            Chain::Fantom => "fantom",
```

Do not add a replacement variant, alias, RPC endpoint, or chain-ID mapping.

- [ ] **Step 4: Remove the Fantom parser selector**

In `crates/core/src/interpreter/frontend/productions.pest`, delete only this alternative from the `chain` production:

```diff
     "ronin" |
-    "fantom" |
     "kava" |
```

- [ ] **Step 5: Remove the bundled RPC and supported-network documentation**

Delete this object from the configuration example in `docs/installation.md`:

```json
"fantom": {
    "default": "https://fantom.drpc.org",
    "rpcs": [
        "https://fantom.drpc.org"
    ]
},
```

Delete the `- Fantom` bullet from the `Pre-configured Networks` list. Leave adjacent Ronin, Kava, and Gnosis entries unchanged.

- [ ] **Step 6: Run focused Rust and parser verification**

Run:

```bash
cargo test -p eql_core common::chain::tests -- --nocapture
cargo test -p eql_core interpreter::frontend::parser::tests
```

Expected: PASS. Four chain tests pass, and all parser tests pass.

- [ ] **Step 7: Verify no supported-surface reference remains**

Run:

```bash
rg -n -i '\bfantom\b|fantom-mainnet|fantom\.drpc\.org' \
  crates/core/src crates/core/tests docs/installation.md
```

Expected: no matches. If `crates/core/tests` does not exist, omit that path and rerun the same search against `crates/core/src` and `docs/installation.md`.

- [ ] **Step 8: Run the core suite and formatting checks**

Run:

```bash
cargo test -p eql_core
cargo fmt --check
git diff --check
```

Expected at this point in the parent Portal plan: the chain and parser coverage pass; the full suite may still report only the two already-recorded later-task failures (`test_get_block_fields` and `test_get_logs`). The modified Rust file is rustfmt-clean and `git diff --check` passes. Record exact output rather than concealing pre-existing failures.

- [ ] **Step 9: Commit**

```bash
git add crates/core/src/common/chain.rs crates/core/src/interpreter/frontend/productions.pest docs/installation.md
git commit -m "feat: remove Fantom chain support"
```
