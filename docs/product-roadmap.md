# Product roadmap

> Living document. Started 2026-07-20 from the product-direction grilling session,
> on top of `docs/evaluation-2026-07-20.md`. This supersedes the sequencing in the
> 2024 alpha `roadmap.md`; language design questions live in `docs/adr/`.

**Direction:** one product for data engineers and analysts — DuckDB-dialect SQL
over chain entities, served by SQD Portal, embeddable into notebook, SQL, and
pipeline workflows. Deliberately no single persona beachhead; sequencing below is
the operative prioritization.

## In flight

1. **Portal hardening** — `portal-primary-routing` + `-sdd` worktree (timeouts,
   shared client, head-snapshot `latest`, pagination guards, routing tests).
2. **DuckDB dialect subset parser** — sqlparser-rs replacing the pest grammar.
   See `docs/adr/0001-duckdb-dialect-subset.md` (desugar rule keeps the
   embedded-DuckDB executor swap open).

## Next: decoded transfers + events (~90 days)

Chosen 2026-07-20. Scope as pinned in the design session (see
`docs/adr/0002-transfers-entity.md` and `CONTEXT.md`):

- **`transfers` entity, all kinds in v1**: erc20/erc721/erc1155 (strict decode,
  mint/burn included), wrap/unwrap (per-chain wrapped-native registry), and
  **native via the Portal traces dataset**. Schedule stance: if traces slip,
  the date slips — scope does not shrink.
- **Amounts raw everywhere; enrichment opt-in**: `--enrich` resolves
  decimals/symbol via contract reads over a pool of free public RPCs
  (disclaimed; bring-your-own RPC for reliability), multicall-batched, with a
  persistent seeded cache. This lands two new capabilities as side effects:
  contract-read machinery and the multi-RPC pool (currently dead config code).
- **Stage 2 (if it fits): generic event decoding** with user-supplied ABI as a
  table function; no third-party ABI fetching.
- **Coverage: capability-aware with loud gaps** (see ADR-0002). Token/wrap
  kinds on all mapped chains; native where traces exist. Unservable kind =
  explicit error; stale dataset head = warning. 2026-07-20 audit: Celo, Mantle,
  Taiko have no traces table; Taiko's head was 49 days stale; Ethereum/Zora
  fresh with full tables. Full 19-chain audit is an early implementation task.

## Guardrail (decided 2026-07-20)

Within 60 days of the hard launch: **≥10 issues/PRs from strangers OR ≥5
unsolicited public usage artifacts** (posts, notebooks, citations). Hit either
→ continue investing. Miss both → a deliberate decision session (archive with
write-up, or one focused pivot) — never drift. This exists because the
2024→2026 dormancy had no tripwire.

## Then: Python + Arrow embed surface (the launch vehicle)

`pip install eql` → `eql.sql("…")` → Arrow/Polars/pandas with streaming
underneath. Serves notebooks directly and is the substrate for dbt/DuckDB
integration; decoded entities ride the same API. Promoted from the deferred
list on 2026-07-20: the hard launch is staked on this surface.

## Launch plan (decided 2026-07-20)

- **Soft launch at transfers (~90 days)**: tagged release (first since
  0.1.4-alpha), rewritten README (DuckDB SQL + transfers + Portal story), no
  publicity push.
- **Hard launch at the Python/Arrow surface**: benchmark-honest blog post
  (vs cryo and raw RPC: setup time, time-to-first-parquet), a properly-done
  Show HN, r/ethdev + crypto-data X + DuckDB/dbt communities, PyPI + brew.
- Posture: fully independent of SQD (decided 2026-07-20 — no adoption pitch);
  consequence accepted that distribution is self-built and Portal free-tier
  risk is unmanaged. Technical hedge to design in: `--portal-url` so
  self-hosted portals keep independence real.

## Deferred (order not yet decided)

- **Resumable extraction + typed Parquet** — checkpointed multi-hour backfills,
  typed (non-string) Parquet schemas, documented schema contract, retry/backoff.
  The data-engineer trust bar.
- **DuckDB extension** — `INSTALL eql; LOAD eql;` chain entities as table
  functions inside DuckDB. Deepest workflow embed; blocked on the executor-swap
  design in ADR-0001.
- **Enrichment metadata as a general capability** (`balanceOf`-style contract
  reads grew from the enrichment machinery).

## Explicit non-goals (from the evaluation)

- Analytics platform / dashboarding (Dune's category).
- Continuous production indexing (pipes-sdk's category — boundary to be pinned).
- App-backend data layer.
