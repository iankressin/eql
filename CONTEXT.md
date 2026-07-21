# EQL

A query language for blockchain data: DuckDB-style SQL over chain entities,
served by SQD Portal with RPC fallback.

## Language

**Entity**:
A queryable blockchain dataset with a fixed schema — `account`, `block`, `tx`,
or `log`. Analogous to a table.
_Avoid_: model, resource, table (reserve "table" for real SQL tables)

**Chain**:
A named blockchain network a query targets; selects where data is fetched from,
not what rows look like.
_Avoid_: network

**Dialect subset**:
The portion of DuckDB SQL that EQL accepts. Anything inside the subset behaves
exactly as SQL says; anything outside is rejected with an explicit error.

**Sugar**:
An EQL-specific first-class notation (bare hex, ENS names, chain names, block
ranges) layered over the dialect subset.
_Avoid_: extension (reserved for DuckDB extensions)

**Desugar rule**:
The constraint that every piece of sugar must rewrite to valid DuckDB SQL by a
purely syntactic transformation.

**ENS name**:
A human-readable name (e.g. `vitalik.eth`) that resolves to an address and is
usable wherever an address is. First-class in the language — a core product
differentiator.

## Transfers

**Transfer**:
A single movement of value, represented as one row classified by a Transfer
kind. Covers token movements (from the universal ERC-20/721/1155 event
signatures, strictly shape-matched), native-coin movements (including internal
ones), and wrap/unwrap movements. Mint and burn legs are Transfers.
_Avoid_: payment, transaction (a transaction may contain many Transfers)

**Transfer kind**:
The classification of a Transfer: token-standard kinds (erc20, erc721,
erc1155), native, wrap, unwrap. Wrap/unwrap are distinct kinds — not native,
not token — precisely so that summing any one kind never double-counts the same
physical movement.

**Mint / Burn**:
A Transfer whose source (mint) or recipient (burn) is the zero address. In
scope as ordinary Transfer rows, filterable — never silently dropped.

**Wrap / Unwrap**:
A deposit into / withdrawal from a wrapped-native contract (WETH-style),
identified against a per-chain registry of wrapped-native contracts.

**Strict decode**:
The stance that only events exactly matching a standard's signature and shape
are decoded. Non-conforming "transfer-like" events stay visible as raw logs but
never become Transfers. A Transfer row is trustworthy or absent — never a
guess.

**Enrichment**:
The opt-in step that augments Transfers with token metadata (scaled amount,
symbol, decimals) resolved from chain state. Raw amounts are always present;
enrichment adds columns, never changes existing ones. Off by default.

**Capability**:
The set of Transfer kinds a chain can serve, determined by which datasets its
data source provides. Capability gaps are loud: asking for a kind a chain
cannot serve is an error, never an empty result.

**Freshness**:
The age of a chain's newest indexed data. Freshness is part of a result's
meaning: results drawn from data older than a threshold carry a warning, so an
absence caused by lag is never mistaken for an absence in the chain.
