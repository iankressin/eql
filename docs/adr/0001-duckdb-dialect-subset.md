# EQL is a DuckDB SQL dialect subset, executed natively

Status: accepted

EQL's bespoke `GET … FROM … ON …` grammar is being replaced by a strict subset of
DuckDB's SQL dialect, parsed with `sqlparser-rs` but executed by the existing
Portal/RPC backend. SQL constructs the backend cannot serve (`JOIN`, `GROUP BY`,
subqueries, …) parse successfully and are rejected at validation with an explicit
"not supported by EQL" error — never a raw syntax error on valid SQL.

**The desugar rule:** every EQL-specific convenience (bare `0x…` literals, ENS
names, block ranges, chain names) must be convertible to valid DuckDB SQL by a
purely syntactic rewrite. No exceptions. This is what keeps the future open: to
gain joins and aggregations, we swap the executor — fetch each entity scan via
Portal, register the results as in-memory DuckDB tables, delegate the relational
work to embedded DuckDB — without changing the language at all.

## Considered options

- **SQL-flavored bespoke grammar** (keep pest, rename keywords): rejected —
  looks like SQL but breaks SQL muscle memory unpredictably; worse than being
  clearly different.
- **Embed DuckDB as parser/engine now**: rejected for now — heavy dependency in
  `core` and the WASM crate, and it forces a fetch-everything-then-filter model
  even for queries Portal answers directly. The desugar rule preserves this as a
  pure executor swap later.
