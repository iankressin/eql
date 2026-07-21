# EQL 2 SQL Dialect Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the pest `GET … FROM … ON …` frontend with a DuckDB-SQL-subset frontend per `docs/adr/0001` and `docs/adr/0002`, keeping the existing Portal/RPC backend.

**Architecture:** A pre-lex pass rewrites value-level sugar (bare hex, ENS, ether units) into plain SQL text; `sqlparser-rs` (DuckDbDialect) parses it; a translation layer converts the SQL AST into the existing `Expression`/`GetExpression`/`Entity` structs, so the backend barely changes. The pest parser survives only to translate legacy queries inside error messages. The language spec is `docs/query.md` (already rewritten).

**Tech Stack:** Rust, sqlparser-rs ~0.52 (DuckDbDialect), existing pest 2.7 (legacy translator only), alloy types, serde/serde_json.

## Global Constraints

- Run tests with `cargo test -p eql_core` from the repo root. All existing tests must stay green unless a task explicitly updates them.
- Frontend tests must not touch the network. Golden tests compare `Expression` structs.
- `sqlparser` minor versions move struct fields (e.g. `Query.limit` vs `limit_clause`). The AST shapes below are for 0.52; if a field name fails to compile, check the installed version's `ast` module — the data is the same, only the field moved.
- Commit per task, conventional-commit style (`feat(sql): …`, `refactor: …`), ending the message with the `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` trailer.
- Public output columns rename: `from` → `from_address`, `to` → `to_address`; transactions gain `block_number`. This is deliberate (ADR 0002) — do not "fix it back".
- Entity names: `accounts`, `blocks`, `transactions` (alias `tx`), `logs`. No other aliases.
- Every "not supported" rejection must name the construct: `NotSupported("JOIN")`, never a generic error.

## File Structure

- Create `crates/core/src/interpreter/frontend/sql/mod.rs` — module root, `EqlSqlError`, `parse_program` entry
- Create `crates/core/src/interpreter/frontend/sql/prelex.rs` — text pre-pass (hex, ENS, units)
- Create `crates/core/src/interpreter/frontend/sql/schema.rs` — entity/field name resolution
- Create `crates/core/src/interpreter/frontend/sql/values.rs` — SQL literal → domain value coercion
- Create `crates/core/src/interpreter/frontend/sql/where_clause.rs` — conjunct split + chain extraction
- Create `crates/core/src/interpreter/frontend/sql/translate.rs` — AST → `Expression`
- Create `crates/core/src/interpreter/frontend/sql/legacy.rs` — GET-query translator for error messages
- Modify `crates/core/src/interpreter/frontend/mod.rs` — add `pub mod sql;`
- Modify `crates/core/src/common/types.rs` — `GetExpression.limit`, `GetExpression.aliases`, `Expression::Set`
- Modify `crates/core/src/common/transaction.rs`, `query_result.rs`, `interpreter/backend/resolve_transaction.rs`, `resolve_portal.rs` — column renames + `block_number`
- Modify `crates/core/src/common/chain.rs`, `config.rs` — session RPC overrides
- Modify `crates/core/src/common/serializer.rs` — aliased JSON dump
- Modify `crates/core/src/interpreter/backend/execution_engine.rs` — `Set` handling, `LIMIT`, aliased dump
- Modify `crates/core/src/interpreter/mod.rs` — swap frontend
- Modify `examples/*.eql`, `README.md` — new syntax

---

### Task 1: Rename from/to output columns, add block_number to transactions

**Files:**
- Modify: `crates/core/src/common/transaction.rs`
- Modify: `crates/core/src/common/query_result.rs`
- Modify: `crates/core/src/interpreter/backend/resolve_transaction.rs`
- Modify: `crates/core/src/interpreter/backend/resolve_portal.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `TransactionField::BlockNumber` variant; `TransactionQueryRes.from_address: Option<Address>`, `.to_address: Option<Address>`, `.block_number: Option<u64>`; Display strings `from_address`, `to_address`, `block_number`. Later tasks (schema.rs, legacy.rs) rely on these exact Display strings.

- [ ] **Step 1: Write the failing tests** (in `transaction.rs` `#[cfg(test)] mod tests` — create the module if absent)

```rust
#[test]
fn transaction_field_display_uses_renamed_columns() {
    assert_eq!(TransactionField::From.to_string(), "from_address");
    assert_eq!(TransactionField::To.to_string(), "to_address");
    assert_eq!(TransactionField::BlockNumber.to_string(), "block_number");
}

#[test]
fn transaction_field_parses_renamed_columns() {
    assert_eq!(TransactionField::try_from("from_address").unwrap(), TransactionField::From);
    assert_eq!(TransactionField::try_from("to_address").unwrap(), TransactionField::To);
    assert_eq!(TransactionField::try_from("block_number").unwrap(), TransactionField::BlockNumber);
    // legacy spellings still parse (used by the legacy translator)
    assert_eq!(TransactionField::try_from("from").unwrap(), TransactionField::From);
    assert_eq!(TransactionField::try_from("to").unwrap(), TransactionField::To);
}
```

And in `query_result.rs` tests:

```rust
#[test]
fn transaction_res_serializes_renamed_keys() {
    let mut res = TransactionQueryRes::default();
    res.from_address = Some(Address::ZERO);
    res.to_address = Some(Address::ZERO);
    res.block_number = Some(1u64);
    let json = serde_json::to_value(&res).unwrap();
    assert!(json.get("from_address").is_some());
    assert!(json.get("to_address").is_some());
    assert!(json.get("block_number").is_some());
    assert!(json.get("from").is_none());
    assert!(json.get("to").is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core transaction_field_display transaction_field_parses transaction_res_serializes`
Expected: FAIL (no `BlockNumber` variant, no `from_address` field).

- [ ] **Step 3: Implement**

In `transaction.rs`:
1. Add `BlockNumber,` to `pub enum TransactionField` (the `EnumVariants` derive picks it up for `all_variants()`).
2. In `impl Display for TransactionField` change the two arms and add one:
   `From => write!(f, "from_address")`, `To => write!(f, "to_address")`, `BlockNumber => write!(f, "block_number")`.
3. In `TryFrom<&str> for TransactionField` accept `"from_address" | "from"` → `From`, `"to_address" | "to"` → `To`, add `"block_number"` → `BlockNumber`.
4. In `Transaction::filter()` the match reads `tx.from` / `tx.to` — rename to `tx.from_address` / `tx.to_address` after step 4 below.

In `query_result.rs` (`TransactionQueryRes`):
1. Rename struct fields `from` → `from_address`, `to` → `to_address`; add `pub block_number: Option<u64>,`.
2. Update `Default`/`new` initializers and the `has_value`-style check (`self.from.is_some()` → `self.from_address.is_some()`, add `|| self.block_number.is_some()`).
3. In the custom `Serialize` impl, change `fields.push(("from", …))` → `("from_address", …)`, `("to", …)` → `("to_address", …)`, and add:

```rust
if let Some(block_number) = &self.block_number {
    fields.push(("block_number", Some(block_number.to_string())));
}
```

Then run `cargo check -p eql_core` and fix every constructor site the compiler flags in `resolve_transaction.rs` and `resolve_portal.rs`: rename the two fields, and populate `block_number` from the source data (alloy's transaction carries `block_number: Option<u64>`; Portal rows carry the block header number — both resolvers already have the value in scope where they build `TransactionQueryRes`). Where the resolver projects requested fields, add a `TransactionField::BlockNumber` arm mirroring the existing per-field arms.

- [ ] **Step 4: Run the full suite**

Run: `cargo test -p eql_core`
Expected: PASS (existing tests that asserted `"from"`/`"to"` keys must be updated to the new names — they are part of this rename).

- [ ] **Step 5: Commit**

```bash
git add -A crates/core
git commit -m "refactor(core)!: rename from/to columns, add transactions.block_number"
```

---

### Task 2: Add sqlparser and the prelex pass

**Files:**
- Modify: `crates/core/Cargo.toml`
- Create: `crates/core/src/interpreter/frontend/sql/mod.rs`
- Create: `crates/core/src/interpreter/frontend/sql/prelex.rs`
- Modify: `crates/core/src/interpreter/frontend/mod.rs` (add `pub mod sql;`)

**Interfaces:**
- Consumes: nothing.
- Produces: `sql::EqlSqlError` (enum used by every later task) and `prelex::prelex(&str) -> Result<String, EqlSqlError>`.

- [ ] **Step 1: Add the dependency**

Run: `cargo add sqlparser@0.52 -p eql_core`
Expected: `sqlparser = "0.52"` in `crates/core/Cargo.toml`.

- [ ] **Step 2: Create `sql/mod.rs` with the error type**

```rust
pub mod prelex;

#[derive(thiserror::Error, Debug)]
pub enum EqlSqlError {
    #[error("SQL parse error: {0}")]
    Parse(String),
    #[error("{0} is not supported by EQL yet")]
    NotSupported(String),
    #[error("{0}")]
    Validation(String),
    #[error("EQL 2 uses SQL syntax. Equivalent:\n\n{suggestion}")]
    LegacySyntax { suggestion: String },
}
```

Add `pub mod sql;` to `crates/core/src/interpreter/frontend/mod.rs`.

- [ ] **Step 3: Write the failing prelex tests** (in `prelex.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::prelex;

    #[test]
    fn quotes_bare_hex() {
        assert_eq!(
            prelex("WHERE address = 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap(),
            "WHERE address = '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045'"
        );
    }

    #[test]
    fn quotes_ens_names_and_subdomains() {
        assert_eq!(prelex("address = vitalik.eth").unwrap(), "address = 'vitalik.eth'");
        assert_eq!(prelex("address = sub.vitalik.eth").unwrap(), "address = 'sub.vitalik.eth'");
    }

    #[test]
    fn leaves_plain_identifiers_alone() {
        assert_eq!(prelex("chain = eth AND number = latest").unwrap(),
                   "chain = eth AND number = latest");
    }

    #[test]
    fn folds_units() {
        assert_eq!(prelex("value > 1 ether").unwrap(), "value > 1000000000000000000");
        assert_eq!(prelex("gas_price < 30 gwei").unwrap(), "gas_price < 30000000000");
        assert_eq!(prelex("value > 1.5 ether").unwrap(), "value > 1500000000000000000");
        assert_eq!(prelex("value = 10 wei").unwrap(), "value = 10");
    }

    #[test]
    fn fractional_wei_is_an_error() {
        assert!(prelex("value > 1.5 wei").is_err());
    }

    #[test]
    fn does_not_touch_strings_or_comments() {
        assert_eq!(
            prelex("sig = 'Transfer(address,address,uint256)' -- 1 ether").unwrap(),
            "sig = 'Transfer(address,address,uint256)' -- 1 ether"
        );
        assert_eq!(prelex("name = 'my-name.eth'").unwrap(), "name = 'my-name.eth'");
    }

    #[test]
    fn hex_inside_in_list() {
        assert_eq!(
            prelex("address IN (0xAb, vitalik.eth)").unwrap(),
            "address IN ('0xAb', 'vitalik.eth')"
        );
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p eql_core prelex`
Expected: FAIL (`prelex` not defined).

- [ ] **Step 5: Implement `prelex.rs`**

```rust
use super::EqlSqlError;

#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),    // [A-Za-z0-9_.] runs (covers numbers, idents, hex, ens)
    Other(String),   // whitespace, operators, punctuation
    Quoted(String),  // complete '…' / "…" / -- … / /* … */ span, verbatim
}

fn tokenize(input: &str) -> Vec<Tok> {
    let chars: Vec<char> = input.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            let quote = c;
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != quote {
                i += 1;
            }
            i = (i + 1).min(chars.len());
            toks.push(Tok::Quoted(chars[start..i].iter().collect()));
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            toks.push(Tok::Quoted(chars[start..i].iter().collect()));
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            let start = i;
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            toks.push(Tok::Quoted(chars[start..i].iter().collect()));
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            toks.push(Tok::Word(chars[start..i].iter().collect()));
        } else {
            let start = i;
            i += 1;
            toks.push(Tok::Other(chars[start..i].iter().collect()));
        }
    }
    toks
}

fn is_hex(w: &str) -> bool {
    w.len() > 2 && w.starts_with("0x") && w[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn is_ens(w: &str) -> bool {
    let mut segs = w.split('.').collect::<Vec<_>>();
    if segs.len() < 2 || segs.pop() != Some("eth") {
        return false;
    }
    segs.iter().all(|s| {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn is_number(w: &str) -> bool {
    !w.is_empty()
        && w.chars().all(|c| c.is_ascii_digit() || c == '.')
        && w.chars().filter(|c| *c == '.').count() <= 1
        && w.chars().any(|c| c.is_ascii_digit())
}

fn unit_multiplier(w: &str) -> Option<u32> {
    // returns the power of ten
    match w.to_ascii_lowercase().as_str() {
        "ether" => Some(18),
        "gwei" => Some(9),
        "wei" => Some(0),
        _ => None,
    }
}

fn fold_unit(number: &str, pow: u32) -> Result<String, EqlSqlError> {
    let (int_part, frac_part) = match number.split_once('.') {
        Some((i, f)) => (i, f),
        None => (number, ""),
    };
    if frac_part.len() as u32 > pow {
        return Err(EqlSqlError::Validation(format!(
            "{number} with this unit is not a whole number of wei"
        )));
    }
    let zeros = pow as usize - frac_part.len();
    let digits = format!("{int_part}{frac_part}{}", "0".repeat(zeros));
    let trimmed = digits.trim_start_matches('0');
    Ok(if trimmed.is_empty() { "0".into() } else { trimmed.into() })
}

pub fn prelex(input: &str) -> Result<String, EqlSqlError> {
    let toks = tokenize(input);
    let mut out = String::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Quoted(s) | Tok::Other(s) => out.push_str(s),
            Tok::Word(w) => {
                // number followed by a unit word (skipping whitespace-only Others)?
                if is_number(w) {
                    let mut j = i + 1;
                    let mut ws = String::new();
                    while let Some(Tok::Other(o)) = toks.get(j) {
                        if o.chars().all(char::is_whitespace) {
                            ws.push_str(o);
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    if let Some(Tok::Word(u)) = toks.get(j) {
                        if let Some(pow) = unit_multiplier(u) {
                            out.push_str(&fold_unit(w, pow)?);
                            i = j + 1;
                            continue;
                        }
                    }
                    let _ = ws; // no unit: fall through, number printed as-is
                }
                if is_hex(w) || is_ens(w) {
                    out.push('\'');
                    out.push_str(w);
                    out.push('\'');
                } else {
                    out.push_str(w);
                }
            }
        }
        i += 1;
    }
    Ok(out)
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p eql_core prelex`
Expected: PASS (8 tests).

- [ ] **Step 7: Commit**

```bash
git add crates/core
git commit -m "feat(sql): add sqlparser dep and prelex pass for hex/ENS/unit sugar"
```

---

### Task 3: Entity and field name resolution (schema.rs)

**Files:**
- Create: `crates/core/src/interpreter/frontend/sql/schema.rs` (add `pub mod schema;` to `sql/mod.rs`)

**Interfaces:**
- Consumes: field enums from `common` (`AccountField`, `BlockField`, `TransactionField`, `LogField`).
- Produces:
  - `pub enum EntityKind { Accounts, Blocks, Transactions, Logs }`
  - `pub fn resolve_entity(name: &str) -> Result<EntityKind, EqlSqlError>`
  - `pub fn resolve_account_field(name: &str) -> Result<AccountField, EqlSqlError>` (same shape for `resolve_block_field`, `resolve_transaction_field`, `resolve_log_field`)

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_entities_and_aliases() {
        assert_eq!(resolve_entity("accounts").unwrap(), EntityKind::Accounts);
        assert_eq!(resolve_entity("TRANSACTIONS").unwrap(), EntityKind::Transactions);
        assert_eq!(resolve_entity("tx").unwrap(), EntityKind::Transactions);
        assert_eq!(resolve_entity("logs").unwrap(), EntityKind::Logs);
        assert_eq!(resolve_entity("blocks").unwrap(), EntityKind::Blocks);
    }

    #[test]
    fn singular_names_get_a_hint() {
        let err = resolve_entity("account").unwrap_err().to_string();
        assert!(err.contains("accounts"), "hint missing: {err}");
    }

    #[test]
    fn resolves_fields_with_aliases() {
        use crate::common::transaction::TransactionField;
        assert_eq!(resolve_transaction_field("from_address").unwrap(), TransactionField::From);
        assert_eq!(resolve_transaction_field("from").unwrap(), TransactionField::From); // quoted "from"
        assert_eq!(resolve_transaction_field("block_number").unwrap(), TransactionField::BlockNumber);
        assert!(resolve_transaction_field("bogus").is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core schema`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement**

```rust
use super::EqlSqlError;
use crate::common::{
    account::AccountField, block::BlockField, logs::LogField, transaction::TransactionField,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EntityKind {
    Accounts,
    Blocks,
    Transactions,
    Logs,
}

pub fn resolve_entity(name: &str) -> Result<EntityKind, EqlSqlError> {
    match name.to_ascii_lowercase().as_str() {
        "accounts" => Ok(EntityKind::Accounts),
        "blocks" => Ok(EntityKind::Blocks),
        "transactions" | "tx" => Ok(EntityKind::Transactions),
        "logs" => Ok(EntityKind::Logs),
        "account" => Err(unknown_entity(name, "accounts")),
        "block" => Err(unknown_entity(name, "blocks")),
        "transaction" | "txs" => Err(unknown_entity(name, "transactions")),
        "log" => Err(unknown_entity(name, "logs")),
        _ => Err(EqlSqlError::Validation(format!(
            "unknown entity '{name}'; expected accounts, blocks, transactions (tx) or logs"
        ))),
    }
}

fn unknown_entity(got: &str, want: &str) -> EqlSqlError {
    EqlSqlError::Validation(format!("unknown entity '{got}'; did you mean '{want}'?"))
}

pub fn resolve_account_field(name: &str) -> Result<AccountField, EqlSqlError> {
    match name.to_ascii_lowercase().as_str() {
        "address" => Ok(AccountField::Address),
        "nonce" => Ok(AccountField::Nonce),
        "balance" => Ok(AccountField::Balance),
        "code" => Ok(AccountField::Code),
        "chain" => Ok(AccountField::Chain),
        _ => Err(unknown_field("accounts", name)),
    }
}

pub fn resolve_block_field(name: &str) -> Result<BlockField, EqlSqlError> {
    match name.to_ascii_lowercase().as_str() {
        "number" => Ok(BlockField::Number),
        "timestamp" => Ok(BlockField::Timestamp),
        "size" => Ok(BlockField::Size),
        "hash" => Ok(BlockField::Hash),
        "parent_hash" => Ok(BlockField::ParentHash),
        "state_root" => Ok(BlockField::StateRoot),
        "transactions_root" => Ok(BlockField::TransactionsRoot),
        "receipts_root" => Ok(BlockField::ReceiptsRoot),
        "logs_bloom" => Ok(BlockField::LogsBloom),
        "extra_data" => Ok(BlockField::ExtraData),
        "mix_hash" => Ok(BlockField::MixHash),
        "total_difficulty" => Ok(BlockField::TotalDifficulty),
        "base_fee_per_gas" => Ok(BlockField::BaseFeePerGas),
        "withdrawals_root" => Ok(BlockField::WithdrawalsRoot),
        "blob_gas_used" => Ok(BlockField::BlobGasUsed),
        "excess_blob_gas" => Ok(BlockField::ExcessBlobGas),
        "parent_beacon_block_root" => Ok(BlockField::ParentBeaconBlockRoot),
        "chain" => Ok(BlockField::Chain),
        _ => Err(unknown_field("blocks", name)),
    }
}

pub fn resolve_transaction_field(name: &str) -> Result<TransactionField, EqlSqlError> {
    match name.to_ascii_lowercase().as_str() {
        "type" => Ok(TransactionField::Type),
        "hash" => Ok(TransactionField::Hash),
        "from_address" | "from" => Ok(TransactionField::From),
        "to_address" | "to" => Ok(TransactionField::To),
        "data" => Ok(TransactionField::Data),
        "value" => Ok(TransactionField::Value),
        "block_number" => Ok(TransactionField::BlockNumber),
        "gas_price" => Ok(TransactionField::GasPrice),
        "gas_limit" => Ok(TransactionField::GasLimit),
        "effective_gas_price" => Ok(TransactionField::EffectiveGasPrice),
        "status" => Ok(TransactionField::Status),
        "chain_id" => Ok(TransactionField::ChainId),
        "v" => Ok(TransactionField::V),
        "r" => Ok(TransactionField::R),
        "s" => Ok(TransactionField::S),
        "max_fee_per_blob_gas" => Ok(TransactionField::MaxFeePerBlobGas),
        "max_fee_per_gas" => Ok(TransactionField::MaxFeePerGas),
        "max_priority_fee_per_gas" => Ok(TransactionField::MaxPriorityFeePerGas),
        "y_parity" => Ok(TransactionField::YParity),
        "authorization_list" => Ok(TransactionField::AuthorizationList),
        "chain" => Ok(TransactionField::Chain),
        _ => Err(unknown_field("transactions", name)),
    }
}

pub fn resolve_log_field(name: &str) -> Result<LogField, EqlSqlError> {
    match name.to_ascii_lowercase().as_str() {
        "address" => Ok(LogField::Address),
        "topic0" => Ok(LogField::Topic0),
        "topic1" => Ok(LogField::Topic1),
        "topic2" => Ok(LogField::Topic2),
        "topic3" => Ok(LogField::Topic3),
        "data" => Ok(LogField::Data),
        "block_hash" => Ok(LogField::BlockHash),
        "block_number" => Ok(LogField::BlockNumber),
        "block_timestamp" => Ok(LogField::BlockTimestamp),
        "transaction_hash" => Ok(LogField::TransactionHash),
        "transaction_index" => Ok(LogField::TransactionIndex),
        "log_index" => Ok(LogField::LogIndex),
        "removed" => Ok(LogField::Removed),
        "chain" => Ok(LogField::Chain),
        _ => Err(unknown_field("logs", name)),
    }
}

fn unknown_field(entity: &str, field: &str) -> EqlSqlError {
    EqlSqlError::Validation(format!("unknown field '{field}' on {entity}"))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core schema`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(sql): entity and field name resolution with aliases"
```

---

### Task 4: Literal coercion (values.rs)

**Files:**
- Create: `crates/core/src/interpreter/frontend/sql/values.rs` (add `pub mod values;` to `sql/mod.rs`)

**Interfaces:**
- Consumes: `sqlparser::ast::{Expr, Value}`; alloy types; `NameOrAddress` from `common::ens`.
- Produces (all `pub`, all returning `Result<_, EqlSqlError>`):
  - `expr_as_string(&Expr) -> Result<String, _>` — string literal or bare identifier text
  - `parse_address(&Expr) -> Result<Address, _>`
  - `parse_name_or_address(&Expr) -> Result<NameOrAddress, _>`
  - `parse_b256(&Expr) -> Result<B256, _>`
  - `parse_u64 / parse_u128 / parse_u256 / parse_u8 / parse_bool`
  - `parse_block_number_or_tag(&Expr) -> Result<BlockNumberOrTag, _>`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::{Expr, Ident, Value};

    fn s(v: &str) -> Expr { Expr::Value(Value::SingleQuotedString(v.into())) }
    fn n(v: &str) -> Expr { Expr::Value(Value::Number(v.into(), false)) }
    fn ident(v: &str) -> Expr { Expr::Identifier(Ident::new(v)) }

    #[test]
    fn parses_addresses_and_ens() {
        assert!(matches!(
            parse_name_or_address(&s("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")).unwrap(),
            crate::common::ens::NameOrAddress::Address(_)
        ));
        assert!(matches!(
            parse_name_or_address(&s("vitalik.eth")).unwrap(),
            crate::common::ens::NameOrAddress::Name(_)
        ));
    }

    #[test]
    fn parses_block_tags_and_numbers() {
        use alloy::eips::BlockNumberOrTag;
        assert_eq!(parse_block_number_or_tag(&ident("latest")).unwrap(), BlockNumberOrTag::Latest);
        assert_eq!(parse_block_number_or_tag(&ident("finalized")).unwrap(), BlockNumberOrTag::Finalized);
        assert_eq!(parse_block_number_or_tag(&n("100")).unwrap(), BlockNumberOrTag::Number(100));
    }

    #[test]
    fn parses_numbers() {
        assert_eq!(parse_u64(&n("42")).unwrap(), 42);
        assert_eq!(parse_u256(&n("1000000000000000000")).unwrap().to_string(), "1000000000000000000");
    }

    #[test]
    fn wrong_shapes_error() {
        assert!(parse_address(&s("not-hex")).is_err());
        assert!(parse_u64(&s("abc")).is_err());
        assert!(parse_block_number_or_tag(&ident("newest")).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core values`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement**

```rust
use super::EqlSqlError;
use crate::common::ens::NameOrAddress;
use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, B256, U256};
use sqlparser::ast::{Expr, Value};
use std::str::FromStr;

pub fn expr_as_string(expr: &Expr) -> Result<String, EqlSqlError> {
    match expr {
        Expr::Value(Value::SingleQuotedString(s)) => Ok(s.clone()),
        Expr::Identifier(ident) => Ok(ident.value.clone()),
        other => Err(EqlSqlError::Validation(format!(
            "expected a string or identifier, got {other}"
        ))),
    }
}

pub fn parse_address(expr: &Expr) -> Result<Address, EqlSqlError> {
    let s = expr_as_string(expr)?;
    Address::from_str(&s)
        .map_err(|e| EqlSqlError::Validation(format!("invalid address '{s}': {e}")))
}

pub fn parse_name_or_address(expr: &Expr) -> Result<NameOrAddress, EqlSqlError> {
    let s = expr_as_string(expr)?;
    if s.ends_with(".eth") {
        Ok(NameOrAddress::Name(s))
    } else {
        Ok(NameOrAddress::Address(Address::from_str(&s).map_err(
            |e| EqlSqlError::Validation(format!("invalid address '{s}': {e}")),
        )?))
    }
}

pub fn parse_b256(expr: &Expr) -> Result<B256, EqlSqlError> {
    let s = expr_as_string(expr)?;
    B256::from_str(&s).map_err(|e| EqlSqlError::Validation(format!("invalid hash '{s}': {e}")))
}

fn number_text(expr: &Expr) -> Result<String, EqlSqlError> {
    match expr {
        Expr::Value(Value::Number(n, _)) => Ok(n.clone()),
        other => Err(EqlSqlError::Validation(format!("expected a number, got {other}"))),
    }
}

pub fn parse_u8(expr: &Expr) -> Result<u8, EqlSqlError> {
    number_text(expr)?.parse().map_err(|e| EqlSqlError::Validation(format!("{e}")))
}

pub fn parse_u64(expr: &Expr) -> Result<u64, EqlSqlError> {
    number_text(expr)?.parse().map_err(|e| EqlSqlError::Validation(format!("{e}")))
}

pub fn parse_u128(expr: &Expr) -> Result<u128, EqlSqlError> {
    number_text(expr)?.parse().map_err(|e| EqlSqlError::Validation(format!("{e}")))
}

pub fn parse_u256(expr: &Expr) -> Result<U256, EqlSqlError> {
    U256::from_str(&number_text(expr)?).map_err(|e| EqlSqlError::Validation(format!("{e}")))
}

pub fn parse_bool(expr: &Expr) -> Result<bool, EqlSqlError> {
    match expr {
        Expr::Value(Value::Boolean(b)) => Ok(*b),
        other => Err(EqlSqlError::Validation(format!("expected true/false, got {other}"))),
    }
}

pub fn parse_block_number_or_tag(expr: &Expr) -> Result<BlockNumberOrTag, EqlSqlError> {
    match expr {
        Expr::Value(Value::Number(_, _)) => Ok(BlockNumberOrTag::Number(parse_u64(expr)?)),
        Expr::Identifier(ident) => match ident.value.to_ascii_lowercase().as_str() {
            "latest" => Ok(BlockNumberOrTag::Latest),
            "earliest" => Ok(BlockNumberOrTag::Earliest),
            "pending" => Ok(BlockNumberOrTag::Pending),
            "finalized" => Ok(BlockNumberOrTag::Finalized),
            "safe" => Ok(BlockNumberOrTag::Safe),
            other => Err(EqlSqlError::Validation(format!(
                "invalid block tag '{other}'; expected latest, earliest, pending, finalized or safe"
            ))),
        },
        other => Err(EqlSqlError::Validation(format!(
            "expected a block number or tag, got {other}"
        ))),
    }
}
```

Note: check the import path for `BlockNumberOrTag` against what `common/block.rs` uses (it already imports it) and reuse the same path.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core values`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(sql): literal coercion from SQL exprs to domain values"
```

---

### Task 5: WHERE decomposition and chain extraction (where_clause.rs)

**Files:**
- Create: `crates/core/src/interpreter/frontend/sql/where_clause.rs` (add `pub mod where_clause;` to `sql/mod.rs`)

**Interfaces:**
- Consumes: `values::expr_as_string`; `Chain`, `ChainOrRpc` from `common::chain`.
- Produces:
  - `pub enum CondOp { Eq, Neq, Gt, Gte, Lt, Lte, In, Between }`
  - `pub struct Condition { pub column: String, pub op: CondOp, pub values: Vec<sqlparser::ast::Expr> }` (`Between` carries exactly two values: low, high)
  - `pub fn split_conditions(selection: Option<&Expr>) -> Result<Vec<Condition>, EqlSqlError>` — flattens `AND`; rejects `OR`/`NOT`/anything else with `NotSupported`
  - `pub fn extract_chains(conds: &mut Vec<Condition>) -> Result<Vec<ChainOrRpc>, EqlSqlError>` — removes and returns the chain conditions; errors if none

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::DuckDbDialect;
    use sqlparser::parser::Parser;

    fn where_of(sql: &str) -> Option<sqlparser::ast::Expr> {
        let stmts = Parser::parse_sql(&DuckDbDialect {}, sql).unwrap();
        match stmts.into_iter().next().unwrap() {
            sqlparser::ast::Statement::Query(q) => match *q.body {
                sqlparser::ast::SetExpr::Select(s) => s.selection.clone(),
                _ => panic!("not a select"),
            },
            _ => panic!("not a query"),
        }
    }

    #[test]
    fn splits_and_conjuncts() {
        let sel = where_of("SELECT a FROM t WHERE x = 1 AND y > 2 AND z IN (1,2)");
        let conds = split_conditions(sel.as_ref()).unwrap();
        assert_eq!(conds.len(), 3);
        assert_eq!(conds[0].column, "x");
        assert_eq!(conds[0].op, CondOp::Eq);
        assert_eq!(conds[1].op, CondOp::Gt);
        assert_eq!(conds[2].op, CondOp::In);
        assert_eq!(conds[2].values.len(), 2);
    }

    #[test]
    fn between_carries_low_and_high() {
        let sel = where_of("SELECT a FROM t WHERE n BETWEEN 1 AND 100");
        let conds = split_conditions(sel.as_ref()).unwrap();
        assert_eq!(conds[0].op, CondOp::Between);
        assert_eq!(conds[0].values.len(), 2);
    }

    #[test]
    fn rejects_or_and_not() {
        let sel = where_of("SELECT a FROM t WHERE x = 1 OR y = 2");
        let err = split_conditions(sel.as_ref()).unwrap_err().to_string();
        assert!(err.contains("OR"));
        let sel = where_of("SELECT a FROM t WHERE NOT x = 1");
        assert!(split_conditions(sel.as_ref()).is_err());
    }

    #[test]
    fn extracts_chains() {
        use crate::common::chain::{Chain, ChainOrRpc};
        let sel = where_of("SELECT a FROM t WHERE chain = eth AND x = 1");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        let chains = extract_chains(&mut conds).unwrap();
        assert_eq!(chains, vec![ChainOrRpc::Chain(Chain::Ethereum)]);
        assert_eq!(conds.len(), 1); // chain condition removed

        let sel = where_of("SELECT a FROM t WHERE chain IN (eth, base)");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        assert_eq!(extract_chains(&mut conds).unwrap().len(), 2);
    }

    #[test]
    fn chain_wildcard_and_url() {
        let sel = where_of("SELECT a FROM t WHERE chain = '*'");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        assert!(extract_chains(&mut conds).unwrap().len() > 5);

        let sel = where_of("SELECT a FROM t WHERE chain = 'https://my-node:8545'");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        assert!(matches!(
            extract_chains(&mut conds).unwrap()[0],
            crate::common::chain::ChainOrRpc::Rpc(_)
        ));
    }

    #[test]
    fn missing_chain_is_an_error() {
        let sel = where_of("SELECT a FROM t WHERE x = 1");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        let err = extract_chains(&mut conds).unwrap_err().to_string();
        assert!(err.contains("chain"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core where_clause`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement**

```rust
use super::{values::expr_as_string, EqlSqlError};
use crate::common::chain::{Chain, ChainOrRpc};
use alloy::transports::http::reqwest::Url;
use sqlparser::ast::{BinaryOperator, Expr};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CondOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Between,
}

#[derive(Debug)]
pub struct Condition {
    pub column: String,
    pub op: CondOp,
    pub values: Vec<Expr>,
}

fn column_name(expr: &Expr) -> Result<String, EqlSqlError> {
    match expr {
        Expr::Identifier(ident) => Ok(ident.value.to_ascii_lowercase()),
        other => Err(EqlSqlError::NotSupported(format!(
            "left side of a condition must be a column name, got {other}"
        ))),
    }
}

pub fn split_conditions(selection: Option<&Expr>) -> Result<Vec<Condition>, EqlSqlError> {
    let mut out = Vec::new();
    if let Some(expr) = selection {
        collect(expr, &mut out)?;
    }
    Ok(out)
}

fn collect(expr: &Expr, out: &mut Vec<Condition>) -> Result<(), EqlSqlError> {
    match expr {
        Expr::BinaryOp { left, op: BinaryOperator::And, right } => {
            collect(left, out)?;
            collect(right, out)
        }
        Expr::BinaryOp { op: BinaryOperator::Or, .. } => {
            Err(EqlSqlError::NotSupported("OR".into()))
        }
        Expr::UnaryOp { .. } | Expr::Not { .. } => Err(EqlSqlError::NotSupported("NOT".into())),
        Expr::Nested(inner) => collect(inner, out),
        Expr::BinaryOp { left, op, right } => {
            let cond_op = match op {
                BinaryOperator::Eq => CondOp::Eq,
                BinaryOperator::NotEq => CondOp::Neq,
                BinaryOperator::Gt => CondOp::Gt,
                BinaryOperator::GtEq => CondOp::Gte,
                BinaryOperator::Lt => CondOp::Lt,
                BinaryOperator::LtEq => CondOp::Lte,
                other => return Err(EqlSqlError::NotSupported(format!("operator {other}"))),
            };
            out.push(Condition {
                column: column_name(left)?,
                op: cond_op,
                values: vec![(**right).clone()],
            });
            Ok(())
        }
        Expr::InList { expr, list, negated } => {
            if *negated {
                return Err(EqlSqlError::NotSupported("NOT IN".into()));
            }
            out.push(Condition {
                column: column_name(expr)?,
                op: CondOp::In,
                values: list.clone(),
            });
            Ok(())
        }
        Expr::Between { expr, negated, low, high } => {
            if *negated {
                return Err(EqlSqlError::NotSupported("NOT BETWEEN".into()));
            }
            out.push(Condition {
                column: column_name(expr)?,
                op: CondOp::Between,
                values: vec![(**low).clone(), (**high).clone()],
            });
            Ok(())
        }
        other => Err(EqlSqlError::NotSupported(format!("condition {other}"))),
    }
}

fn chain_value(expr: &Expr) -> Result<Vec<ChainOrRpc>, EqlSqlError> {
    let text = expr_as_string(expr)?;
    if text == "*" {
        return Chain::from_selector("*")
            .map_err(|e| EqlSqlError::Validation(e.to_string()));
    }
    if text.starts_with("http://") || text.starts_with("https://") {
        let url = Url::parse(&text)
            .map_err(|e| EqlSqlError::Validation(format!("invalid RPC url '{text}': {e}")))?;
        return Ok(vec![ChainOrRpc::Rpc(url)]);
    }
    Chain::try_from(text.as_str())
        .map(|c| vec![ChainOrRpc::Chain(c)])
        .map_err(|e| EqlSqlError::Validation(e.to_string()))
}

pub fn extract_chains(conds: &mut Vec<Condition>) -> Result<Vec<ChainOrRpc>, EqlSqlError> {
    let mut chains: Vec<ChainOrRpc> = Vec::new();
    let mut kept = Vec::new();
    for cond in conds.drain(..) {
        if cond.column == "chain" {
            match cond.op {
                CondOp::Eq | CondOp::In => {
                    for value in &cond.values {
                        chains.extend(chain_value(value)?);
                    }
                }
                _ => {
                    return Err(EqlSqlError::NotSupported(
                        "chain supports only = and IN".into(),
                    ))
                }
            }
        } else {
            kept.push(cond);
        }
    }
    *conds = kept;
    if chains.is_empty() {
        return Err(EqlSqlError::Validation(
            "no target chain; add e.g. AND chain = eth, or chain = '*' for all chains".into(),
        ));
    }
    Ok(chains)
}
```

Note: `Chain::try_from(&str)` exists (the pest path uses it). If `Expr::Not` is not a variant in the installed sqlparser (NOT arrives as `UnaryOp { op: UnaryOperator::Not, .. }`), keep only the `UnaryOp` arm.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core where_clause`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(sql): WHERE decomposition with chain extraction"
```

---

### Task 6: GetExpression extensions + accounts/blocks translation

**Files:**
- Modify: `crates/core/src/common/types.rs`
- Create: `crates/core/src/interpreter/frontend/sql/translate.rs` (add `pub mod translate;` to `sql/mod.rs`)

**Interfaces:**
- Consumes: `schema`, `values`, `where_clause`, entity constructors (`Account::new`, `Block::new`).
- Produces:
  - `GetExpression { entity, chains, dump, limit: Option<usize>, aliases: Option<HashMap<String, String>> }`
  - `translate::statement_to_expression(stmt: &Statement) -> Result<Expression, EqlSqlError>` handling `SELECT` for accounts and blocks (transactions/logs in Task 7, COPY/SET in Task 8).

- [ ] **Step 1: Extend `GetExpression`**

In `types.rs`, add the two fields and update `new`:

```rust
#[derive(Debug, PartialEq)]
pub struct GetExpression {
    pub entity: Entity,
    pub chains: Vec<ChainOrRpc>,
    pub dump: Option<Dump>,
    pub limit: Option<usize>,
    pub aliases: Option<std::collections::HashMap<String, String>>,
}
```

`new` gains `limit` and `aliases` parameters; the pest `TryFrom` passes `None, None`. Run `cargo check -p eql_core` and update every struct-literal construction site the compiler flags (the `execution_engine.rs` test module builds these) with `limit: None, aliases: None`.

- [ ] **Step 2: Write the failing golden tests** (in `translate.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{
        account::{Account, AccountField},
        block::{Block, BlockField, BlockId, BlockRange},
        chain::{Chain, ChainOrRpc},
        ens::NameOrAddress,
        types::Expression,
    };
    use alloy::eips::BlockNumberOrTag;
    use std::str::FromStr;

    fn translate_one(sql: &str) -> Result<Expression, EqlSqlError> {
        let prelexed = crate::interpreter::frontend::sql::prelex::prelex(sql)?;
        let stmts = sqlparser::parser::Parser::parse_sql(
            &sqlparser::dialect::DuckDbDialect {},
            &prelexed,
        )
        .map_err(|e| EqlSqlError::Parse(e.to_string()))?;
        statement_to_expression(&stmts[0])
    }

    #[test]
    fn account_query_translates() {
        let expr = translate_one(
            "SELECT nonce, balance FROM accounts WHERE address = vitalik.eth AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else { panic!("not a Get") };
        assert_eq!(get.chains, vec![ChainOrRpc::Chain(Chain::Ethereum)]);
        assert_eq!(
            get.entity,
            crate::common::entity::Entity::Account(Account::new(
                Some(vec![NameOrAddress::Name("vitalik.eth".into())]),
                None,
                vec![AccountField::Nonce, AccountField::Balance],
            ))
        );
    }

    #[test]
    fn account_in_list_and_wildcard_fields() {
        let expr = translate_one(
            "SELECT * FROM accounts WHERE address IN (0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045, ian.eth) AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else { panic!() };
        let crate::common::entity::Entity::Account(account) = get.entity else { panic!() };
        assert_eq!(account.ids().unwrap().len(), 2);
        assert_eq!(account.fields(), AccountField::all_variants().to_vec());
    }

    #[test]
    fn block_number_eq_between_and_limit() {
        let expr = translate_one(
            "SELECT hash FROM blocks WHERE number BETWEEN 1 AND 100 AND chain = eth LIMIT 5",
        )
        .unwrap();
        let Expression::Get(get) = expr else { panic!() };
        assert_eq!(get.limit, Some(5));
        let crate::common::entity::Entity::Block(block) = get.entity else { panic!() };
        assert_eq!(
            block.ids().unwrap(),
            &vec![BlockId::Range(BlockRange::new(
                BlockNumberOrTag::Number(1),
                Some(BlockNumberOrTag::Number(100)),
            ))]
        );
    }

    #[test]
    fn block_latest_tag() {
        let expr =
            translate_one("SELECT * FROM blocks WHERE number = latest AND chain = eth").unwrap();
        let Expression::Get(get) = expr else { panic!() };
        let crate::common::entity::Entity::Block(block) = get.entity else { panic!() };
        assert_eq!(block.ids().unwrap(), &vec![BlockId::Number(BlockNumberOrTag::Latest)]);
    }

    #[test]
    fn aliases_are_captured() {
        let expr = translate_one(
            "SELECT balance AS eth_balance FROM accounts WHERE address = ian.eth AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else { panic!() };
        assert_eq!(get.aliases.unwrap().get("balance").unwrap(), "eth_balance");
    }

    #[test]
    fn rejects_unsupported_sql() {
        for (sql, needle) in [
            ("SELECT a FROM accounts JOIN blocks ON true WHERE chain = eth", "JOIN"),
            ("SELECT count(*) FROM blocks WHERE number = 1 AND chain = eth", "expression"),
            ("SELECT a FROM blocks WHERE number = 1 AND chain = eth ORDER BY a", "ORDER BY"),
            ("SELECT DISTINCT a FROM blocks WHERE number = 1 AND chain = eth", "DISTINCT"),
            ("SELECT a FROM blocks WHERE number = 1 AND chain = eth GROUP BY a", "GROUP BY"),
        ] {
            let err = translate_one(sql).unwrap_err().to_string();
            assert!(err.contains(needle), "{sql} → {err}");
        }
    }

    #[test]
    fn missing_key_predicate_errors() {
        let err = translate_one("SELECT nonce FROM accounts WHERE chain = eth")
            .unwrap_err()
            .to_string();
        assert!(err.contains("address"));
        let err = translate_one("SELECT hash FROM blocks WHERE chain = eth")
            .unwrap_err()
            .to_string();
        assert!(err.contains("number"));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p eql_core translate`
Expected: FAIL (module missing).

- [ ] **Step 4: Implement `translate.rs` (accounts + blocks)**

```rust
use super::{
    schema::{self, EntityKind},
    values,
    where_clause::{self, CondOp, Condition},
    EqlSqlError,
};
use crate::common::{
    account::{Account, AccountField},
    block::{Block, BlockField, BlockId, BlockRange},
    entity::Entity,
    types::{Expression, GetExpression},
};
use sqlparser::ast::{
    Expr, Select, SelectItem, SetExpr, Statement, TableFactor,
};
use std::collections::HashMap;

pub fn statement_to_expression(stmt: &Statement) -> Result<Expression, EqlSqlError> {
    match stmt {
        Statement::Query(query) => query_to_get(query, None),
        other => Err(EqlSqlError::NotSupported(format!("statement {other}"))),
    }
}

pub(super) fn query_to_get(
    query: &sqlparser::ast::Query,
    dump: Option<crate::common::dump::Dump>,
) -> Result<Expression, EqlSqlError> {
    if query.order_by.is_some() {
        return Err(EqlSqlError::NotSupported("ORDER BY".into()));
    }
    if query.offset.is_some() {
        return Err(EqlSqlError::NotSupported("OFFSET".into()));
    }
    let limit = match &query.limit {
        None => None,
        Some(expr) => Some(values::parse_u64(expr)? as usize),
    };
    let select = match &*query.body {
        SetExpr::Select(select) => select,
        other => Err(EqlSqlError::NotSupported(format!("query form {other}")))?,
    };
    validate_select_shape(select)?;

    let entity_name = table_name(select)?;
    let kind = schema::resolve_entity(&entity_name)?;
    let (field_names, aliases) = projection(select)?;

    let mut conds = where_clause::split_conditions(select.selection.as_ref())?;
    let chains = where_clause::extract_chains(&mut conds)?;

    let entity = match kind {
        EntityKind::Accounts => build_account(&field_names, conds)?,
        EntityKind::Blocks => build_block(&field_names, conds)?,
        EntityKind::Transactions => build_transaction(&field_names, conds)?,
        EntityKind::Logs => build_logs(&field_names, conds)?,
    };

    Ok(Expression::Get(GetExpression {
        entity,
        chains,
        dump,
        limit,
        aliases: if aliases.is_empty() { None } else { Some(aliases) },
    }))
}

fn validate_select_shape(select: &Select) -> Result<(), EqlSqlError> {
    if select.distinct.is_some() {
        return Err(EqlSqlError::NotSupported("DISTINCT".into()));
    }
    if select.having.is_some() {
        return Err(EqlSqlError::NotSupported("HAVING".into()));
    }
    // group_by is GroupByExpr::Expressions(vec, _) when empty in 0.52
    match &select.group_by {
        sqlparser::ast::GroupByExpr::Expressions(exprs, _) if exprs.is_empty() => {}
        _ => return Err(EqlSqlError::NotSupported("GROUP BY".into())),
    }
    if select.from.len() != 1 {
        return Err(EqlSqlError::NotSupported(
            "multiple tables in FROM (JOIN)".into(),
        ));
    }
    if !select.from[0].joins.is_empty() {
        return Err(EqlSqlError::NotSupported("JOIN".into()));
    }
    Ok(())
}

fn table_name(select: &Select) -> Result<String, EqlSqlError> {
    match &select.from[0].relation {
        TableFactor::Table { name, .. } => Ok(name
            .0
            .iter()
            .map(|ident| ident.to_string())
            .collect::<Vec<_>>()
            .join(".")
            .to_ascii_lowercase()),
        other => Err(EqlSqlError::NotSupported(format!("FROM {other}"))),
    }
}

/// Returns (field names in canonical spelling or ["*"], alias map keyed by canonical field name).
fn projection(select: &Select) -> Result<(Vec<String>, HashMap<String, String>), EqlSqlError> {
    let mut names = Vec::new();
    let mut aliases = HashMap::new();
    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => names.push("*".to_string()),
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => {
                names.push(ident.value.to_ascii_lowercase());
            }
            SelectItem::ExprWithAlias { expr: Expr::Identifier(ident), alias } => {
                let name = ident.value.to_ascii_lowercase();
                aliases.insert(name.clone(), alias.value.clone());
                names.push(name);
            }
            other => {
                return Err(EqlSqlError::NotSupported(format!(
                    "SELECT expression '{other}' (only plain fields, * and AS)"
                )))
            }
        }
    }
    Ok((names, aliases))
}

fn build_account(fields: &[String], conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    let fields = if fields == ["*"] {
        AccountField::all_variants().to_vec()
    } else {
        fields
            .iter()
            .map(|f| schema::resolve_account_field(f))
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut ids = Vec::new();
    for cond in conds {
        match (cond.column.as_str(), cond.op) {
            ("address", CondOp::Eq) | ("address", CondOp::In) => {
                for value in &cond.values {
                    ids.push(values::parse_name_or_address(value)?);
                }
            }
            (col, _) => {
                return Err(EqlSqlError::NotSupported(format!(
                    "filter on accounts.{col} (only address = / IN)"
                )))
            }
        }
    }
    if ids.is_empty() {
        return Err(EqlSqlError::Validation(
            "accounts queries need an address predicate (= or IN)".into(),
        ));
    }
    Ok(Entity::Account(Account::new(Some(ids), None, fields)))
}

fn build_block(fields: &[String], conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    let fields = if fields == ["*"] {
        BlockField::all_variants().to_vec()
    } else {
        fields
            .iter()
            .map(|f| schema::resolve_block_field(f))
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut ids = Vec::new();
    for cond in conds {
        match (cond.column.as_str(), cond.op) {
            ("number", CondOp::Eq) | ("number", CondOp::In) => {
                for value in &cond.values {
                    ids.push(BlockId::Number(values::parse_block_number_or_tag(value)?));
                }
            }
            ("number", CondOp::Between) => {
                ids.push(BlockId::Range(BlockRange::new(
                    values::parse_block_number_or_tag(&cond.values[0])?,
                    Some(values::parse_block_number_or_tag(&cond.values[1])?),
                )));
            }
            (col, _) => {
                return Err(EqlSqlError::NotSupported(format!(
                    "filter on blocks.{col} (only number =, IN, BETWEEN)"
                )))
            }
        }
    }
    if ids.is_empty() {
        return Err(EqlSqlError::Validation(
            "blocks queries need a number predicate (=, IN or BETWEEN)".into(),
        ));
    }
    Ok(Entity::Block(Block::new(Some(ids), None, fields)))
}

fn build_transaction(_fields: &[String], _conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    Err(EqlSqlError::NotSupported("transactions (Task 7)".into()))
}

fn build_logs(_fields: &[String], _conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    Err(EqlSqlError::NotSupported("logs (Task 7)".into()))
}
```

Version note: in 0.52 `query.limit` is `Option<Expr>` and `query.order_by` is `Option<OrderBy>`; on other versions adjust the two accesses (`limit_clause`, `order_by.kind`), keeping the same rejections.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p eql_core translate`
Expected: PASS (the two Task-7 stubs are only hit by transaction/log queries, which have no tests yet).

- [ ] **Step 6: Commit**

```bash
git add crates/core
git commit -m "feat(sql): translate accounts and blocks SELECTs into Expression"
```

---

### Task 7: Transactions and logs translation

**Files:**
- Modify: `crates/core/src/interpreter/frontend/sql/translate.rs`

**Interfaces:**
- Consumes: `TransactionFilter`, `FilterType`, `EqualityFilter`, `ComparisonFilter`, `LogFilter`, `Logs::new`, `Transaction::new`.
- Produces: working `build_transaction` and `build_logs`.

- [ ] **Step 1: Write the failing tests** (append to `translate.rs` tests)

```rust
#[test]
fn tx_by_hash() {
    let expr = translate_one(
        "SELECT * FROM tx WHERE hash = 0x6f93d4add2ef6cdfbb9f25b9895830d719dd8edf6637b639d5c33e808ded4247 AND chain = eth",
    )
    .unwrap();
    let Expression::Get(get) = expr else { panic!() };
    let crate::common::entity::Entity::Transaction(tx) = get.entity else { panic!() };
    assert_eq!(tx.ids().unwrap().len(), 1);
}

#[test]
fn tx_by_block_with_value_filter() {
    use crate::common::{
        filters::{ComparisonFilter, FilterType},
        transaction::TransactionFilter,
    };
    use alloy::primitives::U256;
    let expr = translate_one(
        "SELECT from_address, value FROM transactions WHERE block_number = latest AND value > 1 ether AND chain = eth",
    )
    .unwrap();
    let Expression::Get(get) = expr else { panic!() };
    let crate::common::entity::Entity::Transaction(tx) = get.entity else { panic!() };
    let filters = tx.filters().unwrap();
    assert!(filters.iter().any(|f| matches!(f, TransactionFilter::BlockId(_))));
    assert!(filters.iter().any(|f| matches!(
        f,
        TransactionFilter::Value(FilterType::Comparison(ComparisonFilter::Gt(v)))
            if *v == U256::from(10).pow(U256::from(18))
    )));
}

#[test]
fn tx_requires_hash_or_block() {
    let err = translate_one("SELECT value FROM tx WHERE value > 0 AND chain = eth")
        .unwrap_err()
        .to_string();
    assert!(err.contains("block_number") || err.contains("hash"));
}

#[test]
fn tx_ens_in_address_filter_not_supported_yet() {
    let err = translate_one(
        "SELECT value FROM tx WHERE block_number = latest AND from_address = vitalik.eth AND chain = eth",
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("ENS"));
}

#[test]
fn logs_full_filter_set() {
    use crate::common::logs::LogFilter;
    let expr = translate_one(
        "SELECT * FROM logs WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7 \
         AND topic0 = 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a \
         AND block_number BETWEEN 4638657 AND 4638758 AND chain = eth",
    )
    .unwrap();
    let Expression::Get(get) = expr else { panic!() };
    let crate::common::entity::Entity::Logs(logs) = get.entity else { panic!() };
    assert!(logs.filter().iter().any(|f| matches!(f, LogFilter::EmitterAddress(_))));
    assert!(logs.filter().iter().any(|f| matches!(f, LogFilter::Topic0(_))));
    assert!(logs.filter().iter().any(|f| matches!(f, LogFilter::BlockRange(_))));
}

#[test]
fn logs_event_signature_and_required_block() {
    let expr = translate_one(
        "SELECT * FROM logs WHERE event_signature = 'Confirmation(address,uint256)' \
         AND block_number = 4638757 AND chain = eth",
    )
    .unwrap();
    let Expression::Get(get) = expr else { panic!() };
    let crate::common::entity::Entity::Logs(logs) = get.entity else { panic!() };
    assert!(logs
        .filter()
        .iter()
        .any(|f| matches!(f, crate::common::logs::LogFilter::EventSignature(s) if s == "Confirmation(address,uint256)")));

    let err = translate_one(
        "SELECT * FROM logs WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7 AND chain = eth",
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("block"));
}

#[test]
fn logs_reject_non_eq_operators() {
    let err = translate_one(
        "SELECT * FROM logs WHERE topic0 > 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a AND block_number = 1 AND chain = eth",
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("="));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core translate`
Expected: the new tests FAIL against the Task-7 stubs.

- [ ] **Step 3: Implement `build_transaction` and `build_logs`**

Replace the stubs:

```rust
use crate::common::{
    ens::NameOrAddress,
    filters::{ComparisonFilter, EqualityFilter, FilterType},
    logs::{LogField, LogFilter, Logs},
    transaction::{Transaction, TransactionField, TransactionFilter},
};

fn eq_only<T>(op: CondOp, value: T, what: &str) -> Result<EqualityFilter<T>, EqlSqlError> {
    match op {
        CondOp::Eq => Ok(EqualityFilter::Eq(value)),
        CondOp::Neq => Ok(EqualityFilter::Neq(value)),
        _ => Err(EqlSqlError::NotSupported(format!("{what} supports only = and !="))),
    }
}

fn cmp_filter<T>(op: CondOp, value: T, what: &str) -> Result<FilterType<T>, EqlSqlError> {
    Ok(match op {
        CondOp::Eq => FilterType::Equality(EqualityFilter::Eq(value)),
        CondOp::Neq => FilterType::Equality(EqualityFilter::Neq(value)),
        CondOp::Gt => FilterType::Comparison(ComparisonFilter::Gt(value)),
        CondOp::Gte => FilterType::Comparison(ComparisonFilter::Gte(value)),
        CondOp::Lt => FilterType::Comparison(ComparisonFilter::Lt(value)),
        CondOp::Lte => FilterType::Comparison(ComparisonFilter::Lte(value)),
        CondOp::In | CondOp::Between => {
            return Err(EqlSqlError::NotSupported(format!("{what} with IN/BETWEEN")))
        }
    })
}

fn tx_address(cond: &Condition) -> Result<alloy::primitives::Address, EqlSqlError> {
    match values::parse_name_or_address(&cond.values[0])? {
        NameOrAddress::Address(address) => Ok(address),
        NameOrAddress::Name(_) => Err(EqlSqlError::NotSupported(
            "ENS names outside accounts.address".into(),
        )),
    }
}

fn build_transaction(fields: &[String], conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    let fields = if fields == ["*"] {
        TransactionField::all_variants().to_vec()
    } else {
        fields
            .iter()
            .map(|f| schema::resolve_transaction_field(f))
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut ids: Vec<alloy::primitives::B256> = Vec::new();
    let mut filters: Vec<TransactionFilter> = Vec::new();

    for cond in &conds {
        match (cond.column.as_str(), cond.op) {
            ("hash", CondOp::Eq) | ("hash", CondOp::In) => {
                for value in &cond.values {
                    ids.push(values::parse_b256(value)?);
                }
            }
            ("block_number", CondOp::Eq) => filters.push(TransactionFilter::BlockId(
                BlockId::Number(values::parse_block_number_or_tag(&cond.values[0])?),
            )),
            ("block_number", CondOp::Between) => {
                filters.push(TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                    values::parse_block_number_or_tag(&cond.values[0])?,
                    Some(values::parse_block_number_or_tag(&cond.values[1])?),
                ))))
            }
            ("from_address", _) => filters.push(TransactionFilter::From(eq_only(
                cond.op,
                tx_address(cond)?,
                "from_address",
            )?)),
            ("to_address", _) => filters.push(TransactionFilter::To(eq_only(
                cond.op,
                tx_address(cond)?,
                "to_address",
            )?)),
            ("value", _) => filters.push(TransactionFilter::Value(cmp_filter(
                cond.op,
                values::parse_u256(&cond.values[0])?,
                "value",
            )?)),
            ("gas_price", _) => filters.push(TransactionFilter::GasPrice(cmp_filter(
                cond.op,
                values::parse_u128(&cond.values[0])?,
                "gas_price",
            )?)),
            ("gas_limit", _) => filters.push(TransactionFilter::GasLimit(cmp_filter(
                cond.op,
                values::parse_u64(&cond.values[0])?,
                "gas_limit",
            )?)),
            ("effective_gas_price", _) => {
                filters.push(TransactionFilter::EffectiveGasPrice(cmp_filter(
                    cond.op,
                    values::parse_u128(&cond.values[0])?,
                    "effective_gas_price",
                )?))
            }
            ("max_fee_per_gas", _) => filters.push(TransactionFilter::MaxFeePerGas(cmp_filter(
                cond.op,
                values::parse_u128(&cond.values[0])?,
                "max_fee_per_gas",
            )?)),
            ("max_fee_per_blob_gas", _) => {
                filters.push(TransactionFilter::MaxFeePerBlobGas(cmp_filter(
                    cond.op,
                    values::parse_u128(&cond.values[0])?,
                    "max_fee_per_blob_gas",
                )?))
            }
            ("max_priority_fee_per_gas", _) => {
                filters.push(TransactionFilter::MaxPriorityFeePerGas(cmp_filter(
                    cond.op,
                    values::parse_u128(&cond.values[0])?,
                    "max_priority_fee_per_gas",
                )?))
            }
            ("type", _) => filters.push(TransactionFilter::Type(eq_only(
                cond.op,
                values::parse_u8(&cond.values[0])?,
                "type",
            )?)),
            ("status", _) => filters.push(TransactionFilter::Status(eq_only(
                cond.op,
                values::parse_bool(&cond.values[0])?,
                "status",
            )?)),
            ("y_parity", _) => filters.push(TransactionFilter::YParity(eq_only(
                cond.op,
                values::parse_bool(&cond.values[0])?,
                "y_parity",
            )?)),
            (col, _) => {
                return Err(EqlSqlError::NotSupported(format!(
                    "filter on transactions.{col}"
                )))
            }
        }
    }

    let has_block = filters.iter().any(|f| matches!(f, TransactionFilter::BlockId(_)));
    if ids.is_empty() && !has_block {
        return Err(EqlSqlError::Validation(
            "transactions queries need hash (=/IN) or block_number (=/BETWEEN)".into(),
        ));
    }
    Ok(Entity::Transaction(Transaction::new(
        if ids.is_empty() { None } else { Some(ids) },
        if filters.is_empty() { None } else { Some(filters) },
        fields,
    )))
}

fn log_eq(cond: &Condition, what: &str) -> Result<&Expr, EqlSqlError> {
    if cond.op != CondOp::Eq {
        return Err(EqlSqlError::NotSupported(format!(
            "logs.{what} supports only ="
        )));
    }
    Ok(&cond.values[0])
}

fn build_logs(fields: &[String], conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    let fields = if fields == ["*"] {
        LogField::all_variants().to_vec()
    } else {
        fields
            .iter()
            .map(|f| schema::resolve_log_field(f))
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut filters: Vec<LogFilter> = Vec::new();
    for cond in &conds {
        match cond.column.as_str() {
            "address" => filters.push(LogFilter::EmitterAddress(values::parse_address(
                log_eq(cond, "address")?,
            )?)),
            "topic0" => filters.push(LogFilter::Topic0(values::parse_b256(log_eq(cond, "topic0")?)?)),
            "topic1" => filters.push(LogFilter::Topic1(values::parse_b256(log_eq(cond, "topic1")?)?)),
            "topic2" => filters.push(LogFilter::Topic2(values::parse_b256(log_eq(cond, "topic2")?)?)),
            "topic3" => filters.push(LogFilter::Topic3(values::parse_b256(log_eq(cond, "topic3")?)?)),
            "block_hash" => filters.push(LogFilter::BlockHash(values::parse_b256(
                log_eq(cond, "block_hash")?,
            )?)),
            "event_signature" => filters.push(LogFilter::EventSignature(
                values::expr_as_string(log_eq(cond, "event_signature")?)?,
            )),
            "block_number" => match cond.op {
                CondOp::Eq => filters.push(LogFilter::BlockRange(BlockRange::new(
                    values::parse_block_number_or_tag(&cond.values[0])?,
                    None,
                ))),
                CondOp::Between => filters.push(LogFilter::BlockRange(BlockRange::new(
                    values::parse_block_number_or_tag(&cond.values[0])?,
                    Some(values::parse_block_number_or_tag(&cond.values[1])?),
                ))),
                _ => {
                    return Err(EqlSqlError::NotSupported(
                        "logs.block_number supports only = and BETWEEN".into(),
                    ))
                }
            },
            col => return Err(EqlSqlError::NotSupported(format!("filter on logs.{col}"))),
        }
    }

    let has_block = filters
        .iter()
        .any(|f| matches!(f, LogFilter::BlockRange(_) | LogFilter::BlockHash(_)));
    if !has_block {
        return Err(EqlSqlError::Validation(
            "logs queries need block_number (=/BETWEEN) or block_hash".into(),
        ));
    }
    Ok(Entity::Logs(Logs::new(filters, fields)))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core translate`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(sql): translate transactions and logs SELECTs"
```

---

### Task 8: COPY TO, SET rpc_&lt;chain&gt;, and session RPC overrides

**Files:**
- Modify: `crates/core/src/interpreter/frontend/sql/translate.rs`
- Modify: `crates/core/src/common/types.rs`
- Modify: `crates/core/src/common/config.rs`
- Modify: `crates/core/src/common/chain.rs`
- Modify: `crates/core/src/interpreter/backend/execution_engine.rs`

**Interfaces:**
- Consumes: `Dump`, `DumpFormat` (existing), `Chain`, `Url`.
- Produces:
  - `Expression::Set(SetRpcExpression)` with `pub struct SetRpcExpression { pub chain: Chain, pub url: Url }` in `types.rs`
  - `Config::set_session_rpc(chain: &Chain, url: Url)` and `Config::session_rpc(chain: &Chain) -> Option<Url>`
  - `statement_to_expression` handles `Statement::Copy` and `Statement::SetVariable`

- [ ] **Step 1: Write the failing tests** (append to `translate.rs` tests)

```rust
#[test]
fn copy_to_becomes_dump() {
    use crate::common::dump::{Dump, DumpFormat};
    let expr = translate_one(
        "COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO 'out/blocks.parquet'",
    )
    .unwrap();
    let Expression::Get(get) = expr else { panic!() };
    assert_eq!(get.dump, Some(Dump::new("out/blocks".into(), DumpFormat::Parquet)));
}

#[test]
fn copy_rejects_unknown_extension() {
    let err = translate_one(
        "COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO 'out.xlsx'",
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("json") || err.contains("format"));
}

#[test]
fn set_rpc_translates() {
    let expr = translate_one("SET rpc_eth = 'https://my-node:8545'").unwrap();
    let Expression::Set(set) = expr else { panic!("not a Set") };
    assert_eq!(set.chain, crate::common::chain::Chain::Ethereum);
    assert_eq!(set.url.as_str(), "https://my-node:8545/");
}

#[test]
fn set_unknown_variable_errors() {
    assert!(translate_one("SET foo = 'bar'").is_err());
    assert!(translate_one("SET rpc_nochain = 'https://x'").is_err());
}
```

And in `config.rs` tests:

```rust
#[test]
fn session_rpc_override_wins() {
    use crate::common::chain::Chain;
    use alloy::transports::http::reqwest::Url;
    let url = Url::parse("https://session-node:8545").unwrap();
    Config::set_session_rpc(&Chain::Sepolia, url.clone());
    assert_eq!(Config::session_rpc(&Chain::Sepolia), Some(url));
    assert_eq!(Config::session_rpc(&Chain::Gnosis), None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core copy_to set_rpc session_rpc`
Expected: FAIL.

- [ ] **Step 3: Implement**

`types.rs` — add the variant (and derive `PartialEq` like the rest):

```rust
#[derive(Debug, PartialEq)]
pub enum Expression {
    Get(GetExpression),
    Set(SetRpcExpression),
}

#[derive(Debug, PartialEq)]
pub struct SetRpcExpression {
    pub chain: Chain,
    pub url: Url,
}
```

`config.rs` — session override store:

```rust
use std::sync::{Mutex, OnceLock};

static SESSION_RPCS: OnceLock<Mutex<HashMap<Chain, Url>>> = OnceLock::new();

impl Config {
    pub fn set_session_rpc(chain: &Chain, url: Url) {
        SESSION_RPCS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .expect("session rpc lock")
            .insert(chain.clone(), url);
    }

    pub fn session_rpc(chain: &Chain) -> Option<Url> {
        SESSION_RPCS
            .get()?
            .lock()
            .expect("session rpc lock")
            .get(chain)
            .cloned()
    }
}
```

(`Chain` already derives `Eq`; add `Hash` to its derive list for the map key.)

`chain.rs` — consult the override first in `Chain::rpc_url`. If today's signature returns a reference, change it to return an owned `Url` and drop the `.clone()` in `ChainOrRpc::rpc_url`; run `cargo check` and fix any other caller the compiler flags:

```rust
pub fn rpc_url(&self) -> Result<Url> {
    if let Some(url) = Config::session_rpc(self) {
        return Ok(url);
    }
    // existing body, returning an owned Url
}
```

`translate.rs` — extend `statement_to_expression`:

```rust
use crate::common::dump::{Dump, DumpFormat};
use crate::common::types::SetRpcExpression;
use sqlparser::ast::{CopySource, CopyTarget};

pub fn statement_to_expression(stmt: &Statement) -> Result<Expression, EqlSqlError> {
    match stmt {
        Statement::Query(query) => query_to_get(query, None),
        Statement::Copy { source, to: true, target, .. } => {
            let query = match source {
                CopySource::Query(query) => query,
                CopySource::Table { .. } => {
                    return Err(EqlSqlError::NotSupported(
                        "COPY of a bare table; wrap a SELECT: COPY (SELECT …) TO '…'".into(),
                    ))
                }
            };
            let filename = match target {
                CopyTarget::File { filename } => filename.clone(),
                other => return Err(EqlSqlError::NotSupported(format!("COPY TO {other}"))),
            };
            let (name, ext) = filename.rsplit_once('.').ok_or_else(|| {
                EqlSqlError::Validation(
                    "export file needs a .json, .csv or .parquet extension".into(),
                )
            })?;
            let format = DumpFormat::try_from(ext)
                .map_err(|e| EqlSqlError::Validation(e.to_string()))?;
            query_to_get(query, Some(Dump::new(name.to_string(), format)))
        }
        Statement::SetVariable { variables, value, .. } => {
            let variable = variables_single_name(variables)?;
            let chain_name = variable
                .strip_prefix("rpc_")
                .ok_or_else(|| EqlSqlError::NotSupported(format!("SET {variable}")))?;
            let chain = crate::common::chain::Chain::try_from(chain_name)
                .map_err(|e| EqlSqlError::Validation(e.to_string()))?;
            let url_text = values::expr_as_string(value.first().ok_or_else(|| {
                EqlSqlError::Validation("SET needs a value".into())
            })?)?;
            let url = alloy::transports::http::reqwest::Url::parse(&url_text)
                .map_err(|e| EqlSqlError::Validation(format!("invalid url '{url_text}': {e}")))?;
            Ok(Expression::Set(SetRpcExpression { chain, url }))
        }
        other => Err(EqlSqlError::NotSupported(format!("statement {other}"))),
    }
}

fn variables_single_name(
    variables: &sqlparser::ast::OneOrManyWithParens<sqlparser::ast::ObjectName>,
) -> Result<String, EqlSqlError> {
    use sqlparser::ast::OneOrManyWithParens;
    match variables {
        OneOrManyWithParens::One(name) => Ok(name.to_string().to_ascii_lowercase()),
        OneOrManyWithParens::Many(_) => {
            Err(EqlSqlError::NotSupported("SET with multiple variables".into()))
        }
    }
}
```

(`DumpFormat` implements `TryFrom<&str>` — dump.rs uses `as_str().try_into()`. If the trait bound differs, add the one-line impl.)

`execution_engine.rs` — handle the new variant in `run`:

```rust
Expression::Set(set_expr) => {
    crate::common::config::Config::set_session_rpc(&set_expr.chain, set_expr.url.clone());
}
```

(`SET` produces no `QueryResult`; do not push one.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(sql): COPY TO exports and SET rpc_<chain> session overrides"
```

---

### Task 9: LIMIT execution and AS-alias JSON output

**Files:**
- Modify: `crates/core/src/common/query_result.rs`
- Modify: `crates/core/src/common/serializer.rs`
- Modify: `crates/core/src/interpreter/backend/execution_engine.rs`

**Interfaces:**
- Consumes: `GetExpression.limit`, `GetExpression.aliases` (Task 6).
- Produces: `ExpressionResult::truncate(&mut self, n: usize)`; `serializer::dump_results_with_aliases(result, dump, aliases) -> Result<()>`; engine applies both.

- [ ] **Step 1: Write the failing tests**

In `query_result.rs`:

```rust
#[test]
fn truncate_caps_each_variant() {
    let mut res = ExpressionResult::Block(vec![BlockQueryRes::default(); 5]);
    res.truncate(2);
    let ExpressionResult::Block(rows) = &res else { panic!() };
    assert_eq!(rows.len(), 2);
}
```

In `serializer.rs`:

```rust
#[test]
fn aliases_rename_json_keys() {
    let mut value = serde_json::json!([{ "balance": "1", "nonce": "2" }]);
    let aliases = std::collections::HashMap::from([("balance".to_string(), "eth_balance".to_string())]);
    apply_aliases(&mut value, &aliases);
    assert_eq!(value[0].get("eth_balance").unwrap(), "1");
    assert!(value[0].get("balance").is_none());
    assert!(value[0].get("nonce").is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core truncate_caps aliases_rename`
Expected: FAIL.

- [ ] **Step 3: Implement**

`query_result.rs`:

```rust
impl ExpressionResult {
    pub fn truncate(&mut self, n: usize) {
        match self {
            ExpressionResult::Account(v) => v.truncate(n),
            ExpressionResult::Block(v) => v.truncate(n),
            ExpressionResult::Transaction(v) => v.truncate(n),
            ExpressionResult::Log(v) => v.truncate(n),
        }
    }
}
```

`serializer.rs`:

```rust
use std::collections::HashMap;

pub fn apply_aliases(value: &mut serde_json::Value, aliases: &HashMap<String, String>) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                apply_aliases(item, aliases);
            }
        }
        serde_json::Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(alias) = aliases.get(&key) {
                    if let Some(v) = map.remove(&key) {
                        map.insert(alias.clone(), v);
                    }
                }
            }
        }
        _ => {}
    }
}

pub fn dump_results_with_aliases(
    result: &ExpressionResult,
    dump: &Dump,
    aliases: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let mut value = serde_json::to_value(result)?;
    // ExpressionResult serializes as {"account": [...]} etc.; alias the inner rows
    if let serde_json::Value::Object(map) = &mut value {
        for (_, rows) in map.iter_mut() {
            apply_aliases(rows, aliases);
        }
    }
    let rows = value
        .as_object()
        .and_then(|m| m.values().next().cloned())
        .unwrap_or(value);
    std::fs::write(dump.path(), serde_json::to_string_pretty(&rows)?)?;
    Ok(())
}
```

Match the JSON body shape (`rows` extraction) to whatever the existing JSON branch of `dump_results` writes — copy its shape so aliased and plain exports look the same apart from the keys.

`execution_engine.rs`, in `run_get_expr` after resolving `result` and before the dump:

```rust
let mut result = result;
if let Some(limit) = expr.limit {
    result.truncate(limit);
}
if let Some(dump) = &expr.dump {
    match (&expr.aliases, &dump.format) {
        (Some(aliases), crate::common::dump::DumpFormat::Json) => {
            dump_results_with_aliases(&result, dump, aliases)?;
        }
        (Some(_), _) => {
            return Err(crate::interpreter::frontend::sql::EqlSqlError::NotSupported(
                "AS aliases with csv/parquet exports".into(),
            )
            .into())
        }
        (None, _) => {
            let _ = dump_results(&result, dump);
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): LIMIT execution and AS-alias JSON exports"
```

---

### Task 10: Legacy GET translator

**Files:**
- Create: `crates/core/src/interpreter/frontend/sql/legacy.rs` (add `pub mod legacy;` to `sql/mod.rs`)

**Interfaces:**
- Consumes: the pest `Parser` (`frontend::parser::Parser`), `GetExpression` and entity accessors, field `Display` impls.
- Produces: `pub fn legacy_error(source: &str) -> Option<EqlSqlError>` — `Some(LegacySyntax { suggestion })` when the source starts with `GET` (case-insensitive), else `None`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::legacy_error;

    #[test]
    fn non_get_sources_pass_through() {
        assert!(legacy_error("SELECT 1").is_none());
        assert!(legacy_error("  copy (select 1) to 'x.json'").is_none());
    }

    #[test]
    fn account_query_gets_a_suggestion() {
        let err = legacy_error("GET nonce, balance FROM account vitalik.eth ON eth")
            .unwrap()
            .to_string();
        assert!(err.contains("SELECT nonce, balance FROM accounts"), "{err}");
        assert!(err.contains("WHERE address = vitalik.eth"), "{err}");
        assert!(err.contains("chain = eth"), "{err}");
    }

    #[test]
    fn block_range_becomes_between() {
        let err = legacy_error("GET * FROM block 1:100 ON eth").unwrap().to_string();
        assert!(err.contains("SELECT * FROM blocks"), "{err}");
        assert!(err.contains("number BETWEEN 1 AND 100"), "{err}");
    }

    #[test]
    fn tx_and_dump_render() {
        let err = legacy_error(
            "GET from, to FROM tx 0x6f93d4add2ef6cdfbb9f25b9895830d719dd8edf6637b639d5c33e808ded4247 ON eth >> txs.csv",
        )
        .unwrap()
        .to_string();
        assert!(err.contains("from_address, to_address"), "{err}");
        assert!(err.contains("COPY ("), "{err}");
        assert!(err.contains("TO 'txs.csv'"), "{err}");
    }

    #[test]
    fn unparseable_get_still_errors_helpfully() {
        let err = legacy_error("GET gibberish !!!").unwrap().to_string();
        assert!(err.contains("docs/query.md") || err.contains("SQL"), "{err}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core legacy`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement**

```rust
use super::EqlSqlError;
use crate::common::{
    block::{BlockId, BlockRange},
    chain::ChainOrRpc,
    entity::Entity,
    types::{Expression, GetExpression},
};
use crate::interpreter::frontend::parser::Parser;
use alloy::eips::BlockNumberOrTag;

pub fn legacy_error(source: &str) -> Option<EqlSqlError> {
    if !source.trim_start().to_ascii_uppercase().starts_with("GET ") {
        return None;
    }
    let suggestion = match Parser::new(source).parse_expressions() {
        Ok(expressions) => expressions
            .iter()
            .map(|e| match e {
                Expression::Get(get) => render(get),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(";\n"),
        Err(_) => "See docs/query.md for the SQL syntax.".to_string(),
    };
    Some(EqlSqlError::LegacySyntax { suggestion })
}

fn tag(t: &BlockNumberOrTag) -> String {
    match t {
        BlockNumberOrTag::Number(n) => n.to_string(),
        other => format!("{other:?}").to_ascii_lowercase(),
    }
}

fn block_id_condition(column: &str, id: &BlockId) -> String {
    match id {
        BlockId::Number(n) => format!("{column} = {}", tag(n)),
        BlockId::Range(range) => {
            let (start, end) = range.range();
            match end {
                Some(end) => format!("{column} BETWEEN {} AND {}", tag(&start), tag(&end)),
                None => format!("{column} = {}", tag(&start)),
            }
        }
    }
}

fn chains_condition(chains: &[ChainOrRpc]) -> String {
    let rendered: Vec<String> = chains
        .iter()
        .map(|c| match c {
            ChainOrRpc::Chain(chain) => format!("{chain}"),
            ChainOrRpc::Rpc(url) => format!("'{url}'"),
        })
        .collect();
    if rendered.len() == 1 {
        format!("chain = {}", rendered[0])
    } else {
        format!("chain IN ({})", rendered.join(", "))
    }
}

fn render(get: &GetExpression) -> String {
    let (table, fields, mut conditions) = match &get.entity {
        Entity::Account(account) => {
            let fields = account.fields().iter().map(|f| f.to_string()).collect::<Vec<_>>();
            let mut conds = Vec::new();
            if let Some(ids) = account.ids() {
                let rendered: Vec<String> = ids
                    .iter()
                    .map(|id| match id {
                        crate::common::ens::NameOrAddress::Name(n) => n.clone(),
                        crate::common::ens::NameOrAddress::Address(a) => format!("{a}"),
                    })
                    .collect();
                conds.push(if rendered.len() == 1 {
                    format!("address = {}", rendered[0])
                } else {
                    format!("address IN ({})", rendered.join(", "))
                });
            }
            ("accounts", fields, conds)
        }
        Entity::Block(block) => {
            let fields = block.fields().iter().map(|f| f.to_string()).collect::<Vec<_>>();
            let mut conds = Vec::new();
            if let Some(ids) = block.ids() {
                for id in ids {
                    conds.push(block_id_condition("number", id));
                }
            }
            ("blocks", fields, conds)
        }
        Entity::Transaction(tx) => {
            let fields = tx.fields().iter().map(|f| f.to_string()).collect::<Vec<_>>();
            let mut conds = Vec::new();
            if let Some(ids) = tx.ids() {
                let rendered: Vec<String> = ids.iter().map(|h| format!("{h}")).collect();
                conds.push(if rendered.len() == 1 {
                    format!("hash = {}", rendered[0])
                } else {
                    format!("hash IN ({})", rendered.join(", "))
                });
            }
            if let Some(filters) = tx.filters() {
                for filter in filters {
                    if let crate::common::transaction::TransactionFilter::BlockId(id) = filter {
                        conds.push(block_id_condition("block_number", id));
                    }
                }
                if filters.iter().any(|f| {
                    !matches!(f, crate::common::transaction::TransactionFilter::BlockId(_))
                }) {
                    conds.push("-- rewrite your remaining filters as AND conditions".into());
                }
            }
            ("transactions", fields, conds)
        }
        Entity::Logs(logs) => {
            use crate::common::logs::LogFilter;
            let fields = logs.fields().iter().map(|f| f.to_string()).collect::<Vec<_>>();
            let mut conds = Vec::new();
            for filter in logs.filter() {
                conds.push(match filter {
                    LogFilter::EmitterAddress(a) => format!("address = {a}"),
                    LogFilter::Topic0(t) => format!("topic0 = {t}"),
                    LogFilter::Topic1(t) => format!("topic1 = {t}"),
                    LogFilter::Topic2(t) => format!("topic2 = {t}"),
                    LogFilter::Topic3(t) => format!("topic3 = {t}"),
                    LogFilter::BlockHash(h) => format!("block_hash = {h}"),
                    LogFilter::EventSignature(s) => format!("event_signature = '{s}'"),
                    LogFilter::BlockRange(range) => {
                        let (start, end) = range.range();
                        match end {
                            Some(end) => {
                                format!("block_number BETWEEN {} AND {}", tag(&start), tag(&end))
                            }
                            None => format!("block_number = {}", tag(&start)),
                        }
                    }
                });
            }
            ("logs", fields, conds)
        }
    };

    let field_list = if fields.len()
        == match &get.entity {
            Entity::Account(_) => crate::common::account::AccountField::all_variants().len(),
            Entity::Block(_) => crate::common::block::BlockField::all_variants().len(),
            Entity::Transaction(_) => {
                crate::common::transaction::TransactionField::all_variants().len()
            }
            Entity::Logs(_) => crate::common::logs::LogField::all_variants().len(),
        } {
        "*".to_string()
    } else {
        fields.join(", ")
    };

    conditions.push(chains_condition(&get.chains));
    let select = format!(
        "SELECT {field_list} FROM {table}\nWHERE {}",
        conditions.join("\n  AND ")
    );
    match &get.dump {
        Some(dump) => format!("COPY (\n{select}\n) TO '{}'", dump.path()),
        None => select,
    }
}
```

Note: `Chain`'s `Display` must print the short name (`eth`) — it already does for the config lookup; verify with the test and adjust the `format!("{chain}")` to the existing accessor if Display prints the variant name instead.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p eql_core legacy`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(sql): legacy GET queries error with their EQL 2 equivalent"
```

---

### Task 11: Wire the new frontend, update examples and README

**Files:**
- Modify: `crates/core/src/interpreter/frontend/sql/mod.rs`
- Modify: `crates/core/src/interpreter/mod.rs`
- Modify: `examples/get-account.eql`, `examples/get-block.eql`, `examples/get-logs.eql`, `examples/get-transaction.eql`, `examples/query.eql`
- Modify: `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: `sql::parse_program(source: &str) -> Result<Vec<Expression>, EqlSqlError>` — the one frontend entry point.

- [ ] **Step 1: Write the failing integration test** (in `sql/mod.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::parse_program;

    #[test]
    fn parses_a_multi_statement_program() {
        let program = "
            SET rpc_eth = 'https://my-node:8545';
            SELECT nonce, balance FROM accounts
            WHERE address = vitalik.eth AND chain = eth;
            COPY (
              SELECT * FROM logs
              WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7
                AND block_number BETWEEN 4638657 AND 4638758
                AND chain = eth
            ) TO 'usdt.parquet';
        ";
        let expressions = parse_program(program).unwrap();
        assert_eq!(expressions.len(), 3);
    }

    #[test]
    fn legacy_get_reports_equivalent() {
        let err = parse_program("GET balance FROM account vitalik.eth ON eth")
            .unwrap_err()
            .to_string();
        assert!(err.contains("SELECT balance FROM accounts"), "{err}");
    }

    #[test]
    fn join_fails_with_domain_error() {
        let err = parse_program(
            "SELECT * FROM accounts JOIN blocks ON true WHERE address = ian.eth AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("JOIN"), "{err}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p eql_core "sql::"`
Expected: FAIL (`parse_program` missing).

- [ ] **Step 3: Implement `parse_program` and swap the interpreter frontend**

In `sql/mod.rs`:

```rust
pub mod legacy;
pub mod prelex;
pub mod schema;
pub mod translate;
pub mod values;
pub mod where_clause;

use crate::common::types::Expression;
use sqlparser::{dialect::DuckDbDialect, parser::Parser as SqlParser};

pub fn parse_program(source: &str) -> Result<Vec<Expression>, EqlSqlError> {
    if let Some(err) = legacy::legacy_error(source) {
        return Err(err);
    }
    let prelexed = prelex::prelex(source)?;
    let statements = SqlParser::parse_sql(&DuckDbDialect {}, &prelexed)
        .map_err(|e| EqlSqlError::Parse(e.to_string()))?;
    statements
        .iter()
        .map(translate::statement_to_expression)
        .collect()
}
```

In `interpreter/mod.rs`, `run_frontend` becomes:

```rust
fn run_frontend(source: &str) -> Result<Vec<Expression>> {
    let expressions = frontend::sql::parse_program(source)?;
    Ok(expressions)
}
```

(The pest `Parser` stays exported for `legacy.rs`; remove any other external use.)

- [ ] **Step 4: Rewrite the example files** (exact contents)

`examples/get-account.eql`:

```sql
SELECT nonce, balance FROM accounts
WHERE address IN (0x00000000219ab540356cBB839Cbe05303d7705Fa, ian.eth)
  AND chain = eth;
```

`examples/get-block.eql`:

```sql
SELECT hash, size, parent_hash, timestamp FROM blocks
WHERE number = 1 AND chain = eth;
```

`examples/get-logs.eql`:

```sql
SELECT * FROM logs
WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7
  AND topic0 = 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a
  AND block_number BETWEEN 4638657 AND 4638758
  AND chain = eth;

SELECT * FROM logs
WHERE event_signature = 'Confirmation(address,uint256)'
  AND block_number = 4638757
  AND chain = eth;
```

`examples/get-transaction.eql`:

```sql
SELECT authorization_list FROM transactions
WHERE block_number = 45090 AND chain = mekong;
```

`examples/query.eql`:

```sql
SELECT hash, size FROM blocks WHERE number = 1 AND chain = eth;

SELECT hash, size FROM blocks WHERE number BETWEEN 1 AND 10 AND chain = eth;

SELECT nonce, balance FROM accounts
WHERE address = 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045 AND chain = eth;

SELECT from_address, to_address, value, gas_price, status FROM transactions
WHERE hash = 0x6f93d4add2ef6cdfbb9f25b9895830d719dd8edf6637b639d5c33e808ded4247
  AND chain = eth;

SELECT from_address, to_address, value, gas_price, status FROM transactions
WHERE hash IN (
    0x6f93d4add2ef6cdfbb9f25b9895830d719dd8edf6637b639d5c33e808ded4247,
    0xa9e39789f09753e7afa0838c52e3dd332627f1b18eec07e1652ede6f5a958fa1
  )
  AND chain = eth;

SELECT * FROM logs
WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7
  AND topic0 = 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a
  AND block_number BETWEEN 4638657 AND 4638758
  AND chain = eth;

SELECT * FROM logs
WHERE event_signature = 'Confirmation(address,uint256)'
  AND block_number = 4638757
  AND chain = eth;
```

Add a frontend-only test asserting every example file parses:

```rust
#[test]
fn example_files_parse() {
    for file in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples")).unwrap() {
        let path = file.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("eql") {
            let source = std::fs::read_to_string(&path).unwrap();
            parse_program(&source).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        }
    }
}
```

- [ ] **Step 5: Update README examples**

Replace the three `GET` queries in `README.md`:
- Line ~16: `GET balance, balance FROM account vitalik.eth ON eth, base, arbitrum` →
  `SELECT balance FROM accounts WHERE address = vitalik.eth AND chain IN (eth, base, arb)`
- Line ~72: `GET balance, nonce FROM account vitalik.eth ON eth` →
  `SELECT balance, nonce FROM accounts WHERE address = vitalik.eth AND chain = eth`
- Line ~115: `let query = "GET balance FROM account vitalik.eth ON eth";` →
  `let query = "SELECT balance FROM accounts WHERE address = vitalik.eth AND chain = eth";`

Skim the surrounding prose for `GET`/`ON` references and update them to match.

- [ ] **Step 6: Full suite + lint**

Run: `cargo test -p eql_core && cargo clippy -p eql_core -- -D warnings && cargo fmt --check`
Expected: PASS. Fix what fails.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat!: EQL 2 SQL dialect frontend replaces GET syntax"
```

---

## Out of scope (deliberate, per the grilling session)

- `OR`, `NOT`, `ORDER BY`, scalar expressions, `JOIN`, `GROUP BY` — parse-and-reject only.
- ENS resolution outside `accounts.address` — rejected with `NotSupported("ENS names outside accounts.address")`.
- Aliases in CSV/Parquet exports and REPL tables — JSON only for now.
- LIMIT pushdown to Portal — v1 truncates after fetch.
- DuckDB as execution engine — future executor swap per ADR 0001.

## Self-Review Notes

- Spec coverage: statements (T6/T8), entities+schemas (T3/T6/T7), sugar (T2), WHERE rules (T5–T7), chains incl. `'*'`/URL (T5), custom RPC layers (T8), SELECT features (T6/T9), exports (T8/T9), not-supported rejections (T5/T6/T8), migration (T10), renames (T1), examples/README (T11).
- Type consistency: `EqlSqlError` defined once (T2), consumed everywhere; `Condition`/`CondOp` defined in T5, consumed in T6–T7; `GetExpression.limit: Option<usize>`, `aliases: Option<HashMap<String, String>>` defined in T6, consumed in T9; `SetRpcExpression` defined in T8, consumed in T8's engine change; renamed Display strings from T1 consumed by T3 and T10.
