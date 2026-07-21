# EQL 2 language surface: SQL structure, value-level sugar

Status: accepted

EQL 2 replaces `GET … FROM … ON …` with DuckDB SQL. All structural sugar is
gone: chains, entity ids and block ranges move into WHERE; exports use
`COPY (…) TO`; comma-as-AND dies. Sugar survives only at the literal level:
bare hex, bare chain names, bare ENS names (quoted fallback for hyphens and
unicode), block tags, and `<number> <unit>` suffixes — the one lexer extension.

Reserved-word and consistency renames: `from_address` / `to_address` (quoted
`"from"` / `"to"` accepted as aliases), `block_number` on transactions and
logs, plural entity names (`accounts`, `blocks`, `transactions` with alias
`tx`, `logs`). Event signatures are quoted strings only: unquoted identifiers
case-fold, which would silently corrupt the keccak topic0.

v1 executes AND-conjunctions, IN, BETWEEN, comparison operators, AS aliases and
LIMIT. OR, NOT, ORDER BY and scalar expressions parse but return "not yet
supported". `chain` is a required, statically extractable conjunct; `'*'` fans
out to all chains; a URL as the chain value routes that query RPC-only,
bypassing Portal. Custom RPC layers: query URL > `SET rpc_<chain>` > config/CLI.

Old syntax hard-cuts: a query starting with GET runs through the legacy pest
parser once, not to execute but to print the exact new-syntax equivalent in the
error message.

## Consequences

- Every existing query breaks, each with a copy-pasteable fix in its error.
- The language is a strict DuckDB subset, so the planned embedded-DuckDB
  executor (ADR 0001) needs no syntax change — only an executor swap.
