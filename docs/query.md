# EQL 2 Query Reference

EQL 2 is a subset of DuckDB's SQL dialect with first-class notation for
blockchain values. Two rules govern the whole language:

1. **Structure is pure SQL.** Clauses, statements, and operators work exactly
   as DuckDB defines them. SQL that EQL cannot execute yet parses fine and
   fails with a clear `not supported` error — never a syntax error.
2. **Sugar lives in values.** Bare hex, ENS names, chain names, block tags, and
   ether units are EQL-specific. Each rewrites to plain SQL text, so every EQL
   query has an exact DuckDB-SQL equivalent.

## Table of Contents
- [Statements](#statements)
- [Entities](#entities)
- [Values](#values)
- [WHERE Clause](#where-clause)
- [Chains](#chains)
- [SELECT Features](#select-features)
- [Exports](#exports)
- [Not Yet Supported](#not-yet-supported)
- [Migrating from EQL 1](#migrating-from-eql-1)
- [Limitations](#limitations)

## Statements

```sql
SELECT <fields> FROM <entity> WHERE <conditions> [LIMIT <n>];
COPY (<select-statement>) TO '<file>.<ext>';
SET rpc_<chain> = '<url>';
```

Separate statements with `;`. Keywords are case-insensitive.

## Entities

Entities are the queryable datasets. Names are plural; `tx` is an accepted
alias for `transactions`.

### accounts

| Field     | Description                          |
|-----------|--------------------------------------|
| `address` | Account address (query key)          |
| `nonce`   | Transaction count                    |
| `balance` | Balance in wei                       |
| `code`    | Contract bytecode                    |
| `chain`   | Chain the row came from              |

Account queries need an `address` predicate (`=` or `IN`) and a chain.

```sql
SELECT nonce, balance FROM accounts
WHERE address = vitalik.eth AND chain = eth;

SELECT * FROM accounts
WHERE address IN (0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045, ian.eth)
  AND chain IN (eth, base);
```

### blocks

| Field | Description |
|-------|-------------|
| `number` | Block number (query key) |
| `hash` | Block hash |
| `parent_hash` | Parent block hash |
| `timestamp` | Block timestamp |
| `state_root` | State trie root |
| `transactions_root` | Transactions trie root |
| `receipts_root` | Receipts trie root |
| `logs_bloom` | Bloom filter of the block's logs |
| `extra_data` | Arbitrary extra data |
| `mix_hash` | Randomness hash |
| `total_difficulty` | Cumulative difficulty |
| `base_fee_per_gas` | Base fee in wei |
| `withdrawals_root` | Withdrawals trie root |
| `blob_gas_used` | Blob gas used |
| `excess_blob_gas` | Excess blob gas |
| `parent_beacon_block_root` | Parent beacon block root |
| `size` | Block size in bytes |
| `chain` | Chain the row came from |

Block queries need a `number` predicate: `=`, `IN`, or `BETWEEN`. The value can
be a number or a [block tag](#block-tags).

```sql
SELECT * FROM blocks WHERE number = latest AND chain = eth;
SELECT hash, timestamp FROM blocks
WHERE number BETWEEN 1 AND 1000 AND chain = eth;
```

### transactions (alias: tx)

| Field | Description |
|-------|-------------|
| `hash` | Transaction hash (query key) |
| `from_address` | Sender address |
| `to_address` | Recipient address |
| `value` | Value in wei |
| `data` | Input data |
| `block_number` | Block that includes the transaction |
| `gas_price` | Gas price in wei |
| `gas_limit` | Gas limit |
| `effective_gas_price` | Effective gas price in wei |
| `type` | Transaction type |
| `status` | `true` on success, `false` on failure |
| `chain_id` | EIP-155 chain id |
| `v`, `r`, `s` | Signature components |
| `max_fee_per_blob_gas` | EIP-4844 max blob fee |
| `blob_versioned_hashes` | EIP-4844 blob hashes |
| `max_fee_per_gas` | EIP-1559 max fee |
| `max_priority_fee_per_gas` | EIP-1559 priority fee |
| `access_list` | EIP-2930 access list |
| `y_parity` | Signature y parity |
| `authorization_list` | EIP-7702 authorizations |
| `chain` | Chain the row came from |

Quoted `"from"` and `"to"` work as aliases for `from_address` and `to_address`
(they are reserved words in SQL, so they need the quotes).

Transaction queries need either a `hash` predicate (`=` or `IN`) or a
`block_number` predicate (`=` or `BETWEEN`). With a block predicate, other
fields filter the results in memory.

```sql
SELECT * FROM tx
WHERE hash = 0x6f93d4add2ef6cdfbb9f25b9895830d719dd8edf6637b639d5c33e808ded4247
  AND chain = eth;

SELECT from_address, value FROM transactions
WHERE block_number = latest AND value > 1 ether AND chain = eth;
```

### logs

| Field | Description |
|-------|-------------|
| `address` | Emitting contract |
| `topic0` … `topic3` | Log topics |
| `data` | Log data |
| `block_hash` | Hash of the containing block |
| `block_number` | Number of the containing block |
| `block_timestamp` | Timestamp of the containing block |
| `transaction_hash` | Containing transaction |
| `transaction_index` | Transaction position in the block |
| `log_index` | Log position in the block |
| `removed` | `true` if reorged out |
| `chain` | Chain the row came from |

`event_signature` is a filter-only column: writing
`event_signature = 'Transfer(address,address,uint256)'` filters on the
signature's keccak hash (topic0). Write the signature exactly — the hash is
case-sensitive.

Log queries need a `block_number` predicate (`=` or `BETWEEN`) or a
`block_hash` predicate. Log filters support `=` only.

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

## Values

### Hex

Addresses, hashes, topics, and calldata are bare hex — no quotes:
`0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045`.

### ENS names

An ENS name stands in wherever an address literal fits. Plain names and
subdomains work bare; names with hyphens or unicode need quotes:

```sql
WHERE address = vitalik.eth
WHERE address = sub.vitalik.eth
WHERE address = 'my-name.eth'
WHERE address = '🚀🚀🚀.eth'
```

In this version the executor resolves ENS only in `accounts.address`
predicates. Elsewhere an ENS name fails with `not supported yet`.

### Chain names

Bare identifiers: `chain = eth`, `chain IN (eth, base)`. Quoted strings also
work: `chain = 'eth'`.

### Block tags

`latest`, `earliest`, `pending`, `finalized`, `safe` — bare identifiers,
resolved to a concrete block number when the query runs.

### Ether units

A number followed by `ether`, `gwei`, or `wei` folds into wei:

```sql
WHERE value > 1 ether      -- 1000000000000000000
WHERE gas_price < 30 gwei  -- 30000000000
```

Decimals work when the result is a whole number of wei: `1.5 ether` is fine,
`1.5 wei` is an error.

### Strings and booleans

Standard SQL: single-quoted strings, `true` / `false`.

## WHERE Clause

Conditions join with `AND` only. `IN` and `BETWEEN` cover the common
disjunctions; `OR` and `NOT` parse but are not supported yet.

Operators: `=`, `!=` (also `<>`), `>`, `>=`, `<`, `<=`, `IN`, `BETWEEN`.
Log filters support `=` only.

Every query must name its chains, and each entity has a required key predicate
(listed per entity above). Missing either is a validation error, not an empty
result.

## Chains

`chain` is a plain column, but it also routes the query: EQL reads it to pick
the data source before fetching. So chain predicates must sit at the top level
of the WHERE clause — `chain = eth` or `chain IN (eth, base)` joined by `AND`.

Supported chains: `eth`, `sepolia`, `arb`, `op`, `base`, `blast`, `polygon`,
`mantle`, `zksync`, `taiko`, `celo`, `avalanche`, `scroll`, `bnb`, `linea`,
`zora`, `moonbeam`, `moonriver`, `ronin`, `kava`, `gnosis`, `mekong`.

`chain = '*'` fans the query out to every supported chain.

### Custom RPC endpoints

Three levels, most specific wins:

1. **Query**: `chain = 'https://my-node:8545'` — routes that query through the
   given endpoint, RPC only (no Portal). EQL asks the node for its chain id
   and reports the resolved chain name in the `chain` column.
2. **Session**: `SET rpc_eth = 'https://my-node:8545';` — re-points the named
   chain's RPC for the rest of the session. Identity and Portal-first routing
   stay unchanged.
3. **Config**: the `eql-config.json` file, as before.

## SELECT Features

- `*` selects every field.
- `AS` renames output columns: `SELECT balance AS eth_balance …`. In this
  version aliases apply to JSON output and JSON exports; a CSV or Parquet
  export of an aliased query fails with `not supported yet`.
- `LIMIT n` caps the row count.

## Exports

DuckDB's `COPY` writes results to a file. The extension picks the format:
`.json`, `.csv`, or `.parquet`.

```sql
COPY (
  SELECT * FROM logs
  WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7
    AND block_number BETWEEN 4638657 AND 4638758
    AND chain = eth
) TO 'usdt_logs.parquet';
```

File names may contain letters, digits, `-`, `_`, and `/` for subdirectories.

## Not Yet Supported

These parse as valid SQL and fail with a clear error naming the construct:
`OR`, `NOT`, `JOIN`, `GROUP BY`, aggregate functions, subqueries, `ORDER BY`,
`DISTINCT`, `OFFSET`, scalar expressions in SELECT, ENS outside
`accounts.address`, aliases in CSV/Parquet exports.

`ORDER BY` and scalar expressions are next in line. `JOIN` and aggregations
arrive when the DuckDB execution engine lands (see `docs/adr/0001`).

## Migrating from EQL 1

The old `GET … FROM … ON …` syntax is gone. A query starting with `GET` gets
one last parse from the legacy grammar — not to run, but to print the EQL 2
equivalent:

```
> GET balance FROM account vitalik.eth ON eth
ERROR: EQL 2 uses SQL syntax. Equivalent:

  SELECT balance FROM accounts
  WHERE address = vitalik.eth AND chain = eth;
```

| EQL 1 | EQL 2 |
|-------|-------|
| `GET f1, f2 FROM …` | `SELECT f1, f2 FROM …` |
| `FROM account 0xabc…` | `FROM accounts WHERE address = 0xabc…` |
| `FROM account 0xa…, 0xb…` | `WHERE address IN (0xa…, 0xb…)` |
| `FROM block 100` | `FROM blocks WHERE number = 100` |
| `FROM block 1:100` | `WHERE number BETWEEN 1 AND 100` |
| `FROM tx 0xhash…` | `FROM transactions WHERE hash = 0xhash…` |
| `WHERE block = latest` | `WHERE block_number = latest` |
| `from`, `to` | `from_address`, `to_address` |
| `event_signature = Sig(…)` | `event_signature = 'Sig(…)'` |
| `cond1, cond2` | `cond1 AND cond2` |
| `ON eth, base` | `AND chain IN (eth, base)` |
| `ON *` | `AND chain = '*'` |
| `ON https://…` | `AND chain = 'https://…'` |
| `… >> file.csv` | `COPY (…) TO 'file.csv'` |

## Limitations

EQL fetches from SQD Portal first and falls back to JSON-RPC, so:

1. For Portal-served chains, `latest` resolves against the Portal dataset
   head, which can trail the RPC chain tip by a few blocks.
2. RPC rate limits apply when the RPC path is used. Set your own endpoints in
   `eql-config.json` (see the [installation guide](./installation.md)).
