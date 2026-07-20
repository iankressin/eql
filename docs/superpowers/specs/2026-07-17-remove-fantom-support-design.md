# Remove Fantom Support — Design

**Status:** Approved direction; written-spec review pending

## Context

The original Portal-primary routing design retained Fantom as an RPC-only chain after SQD removed the `fantom-mainnet` dataset. The product decision now supersedes that behavior: Fantom is no longer a supported EQL chain at all.

This design supersedes the Fantom-specific requirements in `2026-07-17-portal-primary-routing-design.md` and Task 2 of `2026-07-17-portal-primary-routing.md`.

## Goals

- Remove Fantom from EQL's named chain model and every user-facing selector.
- Reject both the `fantom` selector and chain ID `250` as unsupported.
- Ensure wildcard chain selection no longer includes Fantom.
- Remove the bundled Fantom RPC configuration and supported-chain documentation.
- Leave the generic custom-RPC feature intact for chains that EQL can identify; a custom RPC reporting chain ID `250` is intentionally unsupported.

## Non-Goals

- Do not substitute Sonic (chain ID `146`) for Fantom.
- Do not add a compatibility alias, deprecation period, or Fantom-specific error type.
- Do not remove or redesign generic custom-RPC support.
- Do not rewrite the already-created Git history; the removal lands as a follow-up commit.

## Design

### Domain model and conversions

Delete `Chain::Fantom` from the `Chain` enum. Remove every corresponding exhaustive-match arm:

- Portal dataset mapping
- Default RPC fallback
- string-to-chain conversion
- chain-to-ID conversion
- ID-to-chain conversion
- display conversion

The existing `ChainError::InvalidChain` remains the single unsupported-chain error. As a result, `Chain::try_from("fantom")` and `Chain::try_from(250_u64)` both return `InvalidChain`. Because wildcard selection is generated from `Chain::all_variants()`, removing the variant also removes Fantom from `ON *` without a separate allowlist.

### Parser surface

Remove the `"fantom"` literal from the Pest `chain` production. Queries naming Fantom then fail during parsing like any other unsupported chain rather than reaching a resolver.

### Configuration and documentation

Remove Fantom's bundled RPC entry and Fantom from the supported-chain list in `docs/installation.md`. No unrelated chain configuration changes are included.

### Runtime behavior

Named queries using `fantom` are rejected. RPC URLs that report chain ID `250` are also rejected when EQL resolves the chain identity, which is consistent with full removal. Other named chains and custom RPC URLs for recognized chain IDs behave unchanged.

## Testing

Use TDD in `crates/core/src/common/chain.rs`:

- Assert `Chain::try_from("fantom")` returns `ChainError::InvalidChain`.
- Assert `Chain::try_from(250_u64)` returns `ChainError::InvalidChain`.
- Assert a supported chain still parses and retains its Portal dataset.
- Run the focused chain tests, parser tests that cover chain selectors, and `cargo test -p eql_core`.

The compile-time exhaustiveness of the existing `match` blocks is an additional guard: removing the enum variant forces every Fantom arm to be deleted or compilation fails.

## Success Criteria

- No `Fantom`, `fantom`, `fantom-mainnet`, or Fantom RPC endpoint remains in production code, grammar, configuration documentation, or supported-chain documentation.
- `fantom` and chain ID `250` are rejected as invalid chains.
- Wildcard selection excludes Fantom.
- Existing supported-chain behavior remains green.
