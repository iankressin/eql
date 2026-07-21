# EQL — Honest Evaluation and Real-World Fit

**Subject:** EQL (EVM Query Language) — github.com/iankressin/eql, branch `portal-primary-routing`
**Date:** July 20, 2026
**Method:** Three parallel codebase audits (language surface, data-fetching architecture, engineering health) with file:line verification of every claim, plus adoption-signal collection (GitHub/crates.io/HN/DNS) and a current competitive-landscape review. Headline code claims were spot-verified directly in source.

---

## Verdict

As "a SQL-like language for querying EVM chains," EQL does not have a viable independent market in 2026. Two years of public existence produced 85 stars, a 1-point Hacker News launch, an effectively solo contributor base, and an 18-month dormancy — and during those two years, every job EQL could do got claimed by an entrenched free tool. The bespoke-language premise itself aged badly: LLMs erased the boilerplate pain that was EQL's founding pitch, and MCP servers became the standard way both humans-via-agents and agents themselves query chains.

But the July 2026 Portal migration quietly changed what EQL *is*, and the strategy has not caught up with the code. EQL is no longer "a nicer syntax over JSON-RPC" — it is a zero-configuration terminal client for SQD's 225-network data lake. That narrower thing has exactly one strategically coherent home and one plausible indie lane, both described below. What it cannot remain is what it currently is: a general-purpose language with no users, a README whose headline example doesn't parse, and a published crate that panics on ordinary mainnet data.

## What the code says EQL is

Stripped of framing: one verb (`GET` is the grammar's only statement type — `productions.pest:1`), four entities (account, block, tx, log), projection plus limited filters, over 22 hardcoded chains (19 with Portal datasets; Ronin/Kava/Mekong stay on RPC), with export to JSON/CSV/Parquet. About 6,700 lines of Rust. As of this week, block/tx/log range queries route through SQD Portal with complete field coverage; account state, tx-by-hash, and pending/safe/finalized tags stay on RPC.

What it structurally cannot answer matters more for market fit than what it can:

- **No contract reads.** `balanceOf(...)` sits unchecked on the roadmap. EQL cannot answer "what's this wallet's USDC balance" — the single most common onchain question. Native ETH balance only.
- **No decoding.** Logs return raw topics and data; `event_signature` just keccaks the signature into a topic0 filter. No decoded events, no token semantics, no prices, no labels.
- **No aggregation, joins, or grouping.** `SUM`/`COUNT` never shipped. EQL cannot answer "how many," "total," or "top N" — so the SQL resemblance is syntax-deep. The hard 20% that makes SQL useful for composing datasets (the stated pitch in the original HN post) is the part that's absent.
- **No historical account state, no traces, no state diffs, no receipt fields** (the `status` filter is a near-guaranteed panic — see below).

So the queryable universe is "project fields of raw blocks/txs/logs over ranges." That is extraction, not analysis.

## The adoption evidence

Twenty-five months of data is enough to read: 85 stars, 14 forks. The HN launch got 1 point and 2 comments. Last release 0.1.4-alpha, November 2024 — everything since, including the entire Portal migration, is untagged and uninstallable. Zero commits from December 2024 until July 2026. eql.sh no longer resolves in DNS while the README still advertises it as "Web Mode" (the Vercel deployment is alive). The Discord shows ~55 online. Crates.io shows ~20k downloads of `eql_core`, a metric dominated by mirrors and CI. The market didn't say no — it said nothing, which for a developer tool is the same thing.

## Engineering reality

Being fair in both directions, because both are true:

**The good is real.** The pest grammar and parser are clean and well-tested. Error handling is disciplined thiserror throughout. And the Portal layer built this week is the best engineering in the repo: exhaustive field matching so a new enum variant fails compilation instead of silently downgrading to RPC, live-verified wire formats, server-side pushdown of log and tx filters, and elimination of the per-transaction receipt N+1 that makes the RPC path crawl. The plan document driving it is more rigorous than most professional teams produce.

**The liabilities cap what can honestly be shown to anyone:**

- *First-five-minutes failures.* The README's headline example uses `ON eth, base, arbitrum` — the grammar token is `arb` (`productions.pest:265`), so the first query a visitor copies is a parse error (it also asks for `balance, balance`). The library snippet doesn't compile. The docs use `FROM logs`; the grammar requires `log`. Account `WHERE` filters are broken at AST construction; block `WHERE` parses and then silently returns nothing.
- *Published-crate panics on ordinary data.* `TransactionFilter::filter` unwraps every optional field (`transaction.rs:66–89`). `WHERE to = 0x...` over any range containing a contract creation panics, because `to` is `None` for deployments. Filtering on a field the query didn't select panics. Three independent audits converged on this cluster.
- *No resilience layer.* Zero retries, timeouts, or rate-limit handling anywhere. A stalled Portal connection hangs forever; a 429 on page 7 of 50 discards everything fetched. Pagination, per-block, and per-chain loops are all serial — `ON *` walks 22 chains one at a time. A concrete range past Portal's head silently truncates where the RPC path would error.
- *The RPC range path is a cliff.* `GET * FROM block 1:20000000` materializes 20M block numbers and spawns them as concurrent futures in one `try_join_all` with no semaphore (`resolve_block.rs:320`).
- *Maintenance rot.* `eql_core` compiles against the registry `eql_macros`, not the workspace crate — local edits to the macros silently do nothing. The wasm crate is a 17-line stub with a commented-out license and no packaging. CI is `cargo test` asserting exact historical values against free public RPCs — flaky by construction. `arrow`/`parquet` are pinned at v34 (early 2023). The release workflow pulls actions from `@master` into a `curl | sh` install path, and there's no arm64 build — Apple Silicon users get x86_64 under Rosetta. `docs/repo-structure.md` documents source files that don't exist.

None of these is individually fatal. Collectively they say "prototype," and they mean the current artifact can't be promoted anywhere until the front door is fixed.

## The market, job by job

| Job | Who owns it | Can EQL win? |
|---|---|---|
| Quick lookups while developing | `cast` (ships with Foundry), explorers | No |
| Ad-hoc analytics ("how many, top N") | Dune — free, decoded, real SQL | Structurally excluded |
| Bulk extraction to Parquet/CSV | cryo, HyperSync clients, pipes-sdk | **Credible niche** |
| Data layer inside apps | SDKs, indexers, APIs | No |
| AI-agent data access | MCP servers (incl. SQD's own) | Parity at best |
| Teaching / onboarding | (unowned) | Yes, but tiny |

The prose behind the table:

**Lookups:** EQL's multi-chain one-liner is genuinely nicer than three `cast` invocations. But `cast` is already installed on every EVM developer's machine, and nicer syntax has never displaced an installed default with muscle memory.

**Analytics:** without aggregation and decoding, EQL isn't a weaker Dune competitor — it's not in the category, and positioning language like "compose custom datasets" implies a category it can't enter.

**Apps:** embedding a DSL string in application code is strictly worse than typed SDK calls; note that even Dune is sunsetting Sim, its multichain developer-API platform, on August 1, 2026 — the standalone chain-data-API business is brutal even with Dune's distribution.

**Extraction is the interesting one.** Post-Portal, EQL has a real, honest wedge: *zero configuration*. cryo requires you to bring an RPC endpoint; HyperSync requires client code; EQL turns "give me every Transfer log on Base into a Parquet file" into one memorable line with no signup, no endpoint, no script. That's a genuinely good story for data scientists and researchers. What stands between the story and the reality: no traces or state diffs (Portal *has* both; EQL doesn't expose them), no decoding, all-Parquet-columns-are-strings, and the resilience gaps above — the exact things that separate a demo from a tool data people trust with a 4-hour backfill.

**Agents:** the 2024 premise — boilerplate is painful, a compact DSL helps — inverted. Agents write boilerplate for free, and they consume tool schemas, not ergonomics. Bitquery, The Graph, Chainstack, Blockscout, cryo, and SQD itself all ship MCP servers. The sharpest version of this point: the author's own working environment has the SQD Portal MCP server connected, with richer semantic tools (token transfers, wallet summaries, analytics) than EQL exposes. When the author needs ad-hoc chain data while working, the evidence suggests the reach is for MCP, not for EQL. That dogfood test is the product truth. An `eql` MCP tool is cheap and worth having — a one-line query in an agent transcript is auditable in a way ten tool calls aren't — but it's parity, not a wedge.

## Structural barriers no amount of polish fixes

1. **The bespoke-language tax.** Every user must learn syntax that transfers nowhere, to access a subset of what SQL and `cast` already give them. A DSL earns adoption only when it unlocks something impossible elsewhere; EQL's actual unlock (Portal's reach) is accessible via SDKs and MCP without learning anything.
2. **The semantic gap.** Real questions live above raw entities — tokens, protocols, decoded events, prices. EQL stops below that line, and everything below the line is commoditized to free.
3. **Trust.** A solo-maintainer project that was dormant 18 months is something people try, not something they build on. That's recoverable only via a sponsor or sustained visible cadence.
4. **Platform dependency.** The primary backend is SQD's public endpoint, "free for development." Terms can change. For an independent tool that's existential risk — though for this author specifically, it's the opposite: an argument to make the relationship official.
5. **EVM-only scope** while data gravity is multi-VM — Portal itself serves Solana and Substrate.

## Where it would actually fit

Ranked by realism:

**1. The SQD ecosystem CLI — "psql for Portal."** SQD has 225+ datasets and no human-facing ad-hoc query surface: Portal's front doors are an indexer SDK and an MCP server. A blessed terminal client for the data lake is a real gap; EQL is most of the way there; the author is embedded in that ecosystem and positioned to pitch it. This is the only path where distribution is solved (SQD's docs) and the free-tier dependency becomes a feature. It implies rescoping: dataset names instead of a hardcoded chain enum, traces/state-diffs entities, eventually non-EVM datasets, and hardening the client. In this frame, "85 stars" stops mattering — the value is measured in Portal adoption, not repo stars.

**2. The zero-config extraction tool.** The indie lane: rewrite the pitch from "SQL for EVM" to "one line to Parquet, no RPC needed," benchmark honestly against cryo, harden the data path, add decoding. Viable as a respected niche tool; unlikely to ever be large; requires sustained maintenance that has already once stopped — worth being honest about whether that changed.

**3. A completed learning project.** Also a legitimate outcome. EQL demonstrably shows language-design and systems skill, and plausibly already paid its dividend. Archiving with a candid write-up preserves that value; slow-fading with a broken README erodes it.

**Not viable:** general-purpose query language, analytics platform, app infrastructure, agent-tooling differentiator.

## Recommendation

Pick lane 1 and pitch it internally at SQD; fall back to lane 2 only with a real commitment to cadence; choose lane 3 deliberately rather than by default. Regardless of lane, three things before showing this to anyone: fix the README front door (the example, the dead eql.sh link, the Portal-era architecture story — it still says "maps AST to JSON-RPC methods"), fix the transaction-filter panic cluster and add timeouts/retries, and tag a release — the product people can install today is nine months behind the product that exists. And one thing *not* to do: don't spend another cycle on language features (aggregations, new syntax) before the positioning question is answered, because features added to a tool nobody can find or trust compound nothing.

The uncomfortable summary: the craft is real, the recent engineering is the best in the repo's history, and neither of those is the bottleneck. Distribution and semantics are — and both have been unowned for two years. If EQL gets a home that solves distribution (SQD) or a wedge narrow enough to win outright (zero-config extraction), it fits in the world. As "a query language for EVM chains," the world already answered.

---

## Sources

- [Sunsetting Sim — Dune blog](https://dune.com/blog/sunsetting-sim)
- [cryo — Paradigm](https://github.com/paradigmxyz/cryo) and [cryo-mcp](https://github.com/z80dev/cryo-mcp)
- [Envio HyperSync docs](https://docs.envio.dev/docs/HyperSync/overview)
- [SQD Portal](https://sqd.dev/portal/)
- [The Graph — natural-language MCP querying](https://thegraph.com/blog/querying-blockchain-data-natural-language-mcp-skills/)
- [Bitquery MCP Server](https://bitquery.io/products/bitquery-mcp-server)
- [EQL Hacker News launch thread](https://news.ycombinator.com/item?id=41122762)

## Appendix: Evidence base

- **Codebase:** three parallel audits of branch `portal-primary-routing` at commit `bd84785` (2026-07-20) covering the grammar/language surface, the Portal/RPC data-fetching architecture, and engineering health (tests, CI, docs, dependencies, workspace). All claims carry file:line references; the four headline claims (unparseable README example, space-as-equality grammar quirk, swallowed dump errors, transaction-filter unwrap cluster) were re-verified directly in source.
- **Adoption:** GitHub API (stars/forks/releases), crates.io API, git history (cadence, contributors, tags), HN Algolia API, DNS lookup of eql.sh, Discord presence badge.
- **Market:** July 2026 web review of cast/Foundry, cryo, HyperSync, Dune/Sim, SQD Portal, and the MCP ecosystem for onchain data.
- **Known limitations of this analysis:** static code reading only (no runtime execution of the panic reproductions, though the `Option` semantics are unambiguous); crates.io download counts are not deduplicated for mirrors/CI; X/Twitter traction was not measurable with available tools.
