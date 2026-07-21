# Transfers is a first-class entity with a complete kind taxonomy

Status: draft — decisions ratified in the 2026-07-20 design session; two
proposals below (marked) await ratification.

EQL gains a `transfers` entity: one row per single movement of value, classified
by a `kind` column — `erc20 | erc721 | erc1155 | native | wrap | unwrap`. All
kinds ship in v1, including native, which requires the Portal **traces**
dataset (internal transfers are most native movement; a tx.value-only native
table produces silently wrong sums and is rejected outright). Mint/burn legs
are included as ordinary rows. Wrap/unwrap are their own kinds — not native,
not token — so summing any single kind never double-counts the one physical
movement a WETH deposit represents. Decoding is **strict**: only events exactly
matching the universal standard signatures and shapes become Transfers;
non-conforming lookalikes remain raw logs (a Transfer row is trustworthy or
absent — never a guess).

Amounts are **raw integer base units, always present**. Scaling requires token
`decimals`, i.e. contract reads — a capability EQL doesn't otherwise have.
**Enrichment is opt-in** (`--enrich`): it resolves decimals/symbol via
multicall-batched reads over a pool of free public RPCs (disclaimed;
bring-your-own endpoint for reliability), backed by a persistent local cache
seeded with a shipped snapshot of known tokens. Enrichment adds columns; it
never changes the meaning of existing ones. Columns must mean one thing: no
per-row mixing of raw and scaled values, ever.

## Proposed, unratified

- **Row grain**: one row per transferred asset — `TransferBatch` explodes to one
  row per (id, value) pair; row identity is (block, tx_index, log_index[,
  batch_ordinal]) for log-derived kinds and the trace address path for native.
  This is the grain analysts expect (sums work naturally; matches Dune).
- **Stable schema across flags**: enrichment columns (`amount_scaled`,
  `symbol`, `decimals`) exist in every result and are NULL when enrichment is
  off or metadata is unresolvable. Engineers get one schema regardless of
  flags; analysts discover enrichment by seeing the empty columns.

## Coverage posture (added after Portal capability audit, same session)

A 2026-07-20 audit of Portal datasets found the capability surface is uneven:
e.g. Celo/Mantle/Taiko serve no `traces` table (native transfers impossible
there today) and Taiko's indexed head was 49 days stale. Coverage is therefore
**capability-aware with loud gaps**: token/wrap kinds ship on every mapped
chain, native ships where traces exist, requesting an unservable kind is an
explicit error, and results from a dataset whose head exceeds a staleness
threshold carry a warning. The per-chain capability matrix in docs is
generated, never hand-maintained.

## Considered options

- **Token-only transfers (no native/wrap) in v1**: rejected by the author —
  completeness of the analyst experience was prioritized over shipping speed;
  the accepted consequence is that the date, not the scope, moves if traces
  integration is slow.
- **Decimals-applied amounts by default**: rejected — either drags free-RPC
  fragility into every query or partially normalizes the long tail (a per-row
  meaning footgun).
- **Bundled static token list as the metadata source**: rejected as a semantic
  tier; reborn as the cache seed, which gets the same speed win without a
  curation-dependent meaning for the amount column.
- **Generic ABI decoding as the v1 mechanism (transfers as a preset)**:
  rejected for sequencing — it front-loads ABI sourcing and dynamic-schema
  design before any analyst value ships; it remains stage 2 with user-supplied
  ABIs.
- **Serve whatever exists, silently** (every chain returns the kinds its
  dataset happens to have, no errors or warnings): considered and rejected —
  it contradicts the trust stance ("trustworthy or absent"), turning coverage
  gaps and dataset lag into confident wrong sums (a zero-row native query on a
  traces-less chain; volume charts that flatline when a dataset stalls).
