"# EQL (Ethereum Query Language) Documentation

## Table of Contents

1. [Introduction](#introduction)
2. [Basic Syntax](#basic-syntax)
3. [Entity Types](#entity-types)
4. [Field Selection](#field-selection)
5. [Chain Selection](#chain-selection)
6. [Filters](#filters)
7. [Output Format](#output-format)
8. [Examples](#examples)

## Introduction

EQL is a domain-specific query language designed for retrieving data from Ethereum and other EVM-compatible blockchains. It provides a SQL-like syntax for querying blockchain data including accounts, blocks, transactions, and logs.

## Basic Syntax

The basic structure of an EQL query follows this pattern:

```
GET <fields> FROM <entity> <identifiers/filters> ON <chain>
```

Multiple queries can be separated by commas or semicolons:

```
GET field1 FROM entity1 WHERE condition1 ON chain1;
GET field2 FROM entity2 WHERE condition2 ON chain2
```

## Entity Types

### 1. Account

Query structure for accounts:

```
GET <account_fields> FROM account <address_or_ens>
```

Available fields:

- nonce: Account nonce
- balance: Account balance
- code: Contract code if present
- chain: The chain identifier

Example:

```
GET nonce, balance FROM account 0x1234...5678 ON eth
```

### 2. Block

Query structure for blocks:

```
GET <block_fields> FROM block <block_identifier>
```

Available fields:

- number
- hash
- parent_hash
- timestamp
- state_root
- transactions_root
- receipts_root
- logs_bloom
- extra_data
- mix_hash
- total_difficulty
- base_fee_per_gas
- withdrawals_root
- blob_gas_used
- excess_blob_gas
- parent_beacon_block_root
- size
- chain

Block identifiers can be:

- Single block number: 1234
- Block range: 1234:5678
- Block tags: latest, earliest, pending, finalized, safe

Example:

```
GET timestamp, hash FROM block 1:100 ON eth
```

### 3. Transaction

Query structure for transactions:

```
GET <tx_fields> FROM tx <tx_hash>
```

Available fields:

- transaction_type
- hash
- from
- to
- data
- value
- fee
- gas_price
- gas
- status
- chain_id
- v
- r
- s
- max_fee_per_blob_gas
- blob_versioned_hashes
- max_fee_per_gas
- max_priority_fee_per_gas
- access_list
- y_parity
- chain

### 4. Logs

Query structure for logs:

```
GET <log_fields> FROM log WHERE <filters>
```

Available fields:

- address
- topic0
- topic1
- topic2
- topic3
- data
- block_hash
- block_number
- block_timestamp
- transaction_hash
- transaction_index
- log_index
- removed
- chain

## Chain Selection

Chains can be specified in three ways:

1. Single chain:

```
ON eth
```

2. Multiple chains:

```
ON eth, polygon, arbitrum
```

3. All supported chains:

```
ON 
```

Supported chains:

- eth (Ethereum)
- arb (Arbitrum)
- op (Optimism)
- base
- blast
- polygon
- sepolia
- mantle
- zksync
- taiko
- celo
- avalanche
- scroll
- bnb
- linea
- zora
- moonbeam
- moonriver
- ronin
- fantom
- kava
- gnosis

4. Custom RPC:

```
ON http://localhost:8545
```

## Filters

Filters use a SQL-like WHERE clause syntax. Different entities support different filter types:

### Transaction Filters

- WHERE from = <address>
- WHERE to = <address>
- WHERE value >= <amount>
- WHERE gas_price <= <price>
- WHERE status = true

### Log Filters

- WHERE address = <contract_address>
- WHERE topic0 = <topic_hash>
- WHERE block = <block_number>
- WHERE event_signature = \"Transfer(address,address,uint256)\"

### Block Filters

- WHERE block = <block_number>
- WHERE block = <block_range>

Comparison operators:

- = or space for equality
- != for inequality
- > greater than
- >= greater than or equal
- < less than
- <= less than or equal

## Output Format

Results can be dumped to files using the >> operator:

```
GET fields FROM entity ON chain >> filename.format
```

Supported formats:

- JSON (.json)
- CSV (.csv)
- YAML (.yaml)
- TOML (.toml)
- Parquet (.parquet)

## Examples

1. Query account balance:

```
GET balance FROM account vitalik.eth ON eth
```

2. Query block range with multiple fields:

```
GET number, timestamp, hash FROM block 1:100 ON eth
```

3. Query transaction with filters:

```
GET from, to, value FROM tx WHERE value >= 100 ON eth
```

4. Query logs with event filtering:

```
GET topic0, data FROM log WHERE event_signature = \"Transfer(address,address,uint256)\" ON eth
```

5. Multi-chain query:

```
GET balance FROM account 0x1234...5678 ON eth, polygon, arbitrum
```

6. Export results:

```
GET timestamp FROM block 1:1000 ON eth >> blocks.json
```

This documentation covers the main features of the EQL language as implemented in the codebase. The language is parsed using Pest parser (defined in productions.pest) and executed through the execution engine (execution_engine.rs).

For specific implementation details, refer to the respective source files in the codebase.