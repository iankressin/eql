# Query Syntax Guide

## Table of Contents
- [Basic Syntax](#basic-syntax)
- [Entity Identifiers](#entity-identifiers)
- [Entities](#entities)
  - [Account](#account)
  - [Block](#block)
  - [Transaction](#transaction)
  - [Event Log](#event-log)
- [WHERE Clause](#where-clause)
- [File Exports](#file-exports)
- [Limitations](#limitations)

## Basic Syntax

EQL queries follow this general structure:
```sql
GET <fields> FROM <entity> [WHERE <conditions>] ON <chains>
```

### Components
- `GET`: Specifies the fields you want to retrieve
- `FROM`: Defines the entity type to query
- `WHERE`: (Optional) Filters the results
- `ON`: Specifies target chains

# Entities

Entities are analogus to the tables in a relational database, and are used to query data from. The supported entities are:
- `account`
- `block`
- `tx`
- `log`

### Entity Identifiers
Entities can be queried using:
- Single ID: `GET balance FROM account 0x123... ON eth`
- Multiple IDs: `GET balance FROM account 0x123..., 0x456..., vitalik.eth ON eth`

## Account

### Identifiers
Accounts supports the following identifiers:
- Address: 0x prefixed hex string
- ENS name: A domain name registered on the ENS registry

### Available fields
- `balance`: Current balance in wei
- `nonce`: Transaction count
- `code`: Contract bytecode (if contract account)
- `address`: Account address
- `chain`: Chain identifier (generelly used for cross-chain queries)

### Examples
#### Fetching from a single address
```sql
GET * FROM account 0x123...abc ON eth
```

#### Fetching from multiple addresses
```sql
GET * FROM account 0x123..., 0x456... ON eth
```

#### Fetching from an ENS name
```sql
GET * FROM account vitalik.eth ON eth
```

## Block

### Identifiers
Blocks supports the following identifiers:
- Block number: Integer
- Block hash: 0x prefixed hex string
- Block range: Two integers separated by a colon (`:`), representing the start and end block numbers

### Available fields
- `number`: Block number
- `hash`: Block hash
- `parent_hash`: Parent block hash
- `timestamp`: Block timestamp
- `state_root`: A Merkle root hash of the state of the Ethereum network at a given block, including all account balances, code, and storage.
- `transactions_root`: A Merkle root hash of all transactions included in the block
- `receipts_root`: A Merkle root hash of all receipts included in the block
- `logs_bloom`: A bit vector of size 256 that compactly represents the existence of topics in the logs of a block.
- `extra_data`: Arbitrary data associated with the block, such as a block's metadata or a custom consensus algorithm's parameters.
- `mix_hash`: A random hash used to ensure the uniqueness of the block.
- `total_difficulty`: A cumulative measure of the difficulty of the proof-of-work algorithm that miners must solve to produce a valid block.
- `base_fee_per_gas`: Base fee per gas
- `withdrawals_root`: A Merkle root hash of the withdrawals included in the block
- `blob_gas_used`: The total amount of gas used for blob transactions in the block.
- `excess_blob_gas`: The amount of excess blob gas in the block.
- `parent_beacon_block_root`: The hash of the parent beacon block.
- `size`: Block size in bytes.
- `chain`: Chain identifier

### Examples
#### Fetching the latest block
```sql
GET * FROM block latest ON eth
```

#### Fetching a specific block
```sql
GET * FROM block 17000000 ON eth
```

#### Fetching a block range
```sql
GET * FROM block 1:1000 ON eth
```

## Transaction

### Identifiers
Transactions supports the following identifiers:
- Transaction hash: 0x prefixed hex string

### Available fields
- `hash`: Transaction hash
- `from`: Sender address
- `to`: Recipient address
- `value`: Transaction value in wei
- `data`: Transaction input data 
- `nonce`: Transaction nonce
- `gas_price`: Gas price in wei
- `gas`: Gas limit
- `transaction_type`: Transaction type
- `fee`: Transaction fee in wei
- `status`: Transaction status (true = success, false = failure)
- `v`: v component of signature
- `r`: r component of signature
- `s`: s component of signature
- `chain`: Chain identifier
- `max_fee_per_blob_gas`: Maximum fee per blob gas
- `blob_versioned_hashes`: Blob versioned hashes
- `max_fee_per_gas`: Maximum fee per gas
- `max_priority_fee_per_gas`: Maximum priority fee per gas
- `access_list`: Access list
- `y_parity`: Y parity value

### Examples
#### Fetching single transaction
```sql
GET * FROM tx 0x456... ON eth
```
#### Fetching transactions from a list of addresses
```sql
GET * FROM tx 0x456..., 0x789... ON eth
```
#### Fetching transactions from the latest block
```sql
GET * FROM tx WHERE block = latest ON eth
```

## Event Log Queries

### Identifiers
Different from transactions, blocks and accounts, logs doesn't have a global identifier, instead they are stored in a vector inside of a transaction receipt, therefore in order to fetch logs, we need to specify a combination of fields to filter the logs by.

The filtering is done using the `WHERE` clause, which is explained in more detail [below](#where-clause).

The log queries are mapped to the `eth_getLogs` JSON-RPC method, which requires either a block number or a block range to be specified.

> Log queries do not support list of blocks

> Log queries do not support comparison operators. The only supported operator is `=`.

### Examples
#### Fetching logs using topic and address
```sql
// ERC20 transfer events
GET * FROM logs 
WHERE 
address = 0x123...,
topic0 = 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef,
block = latest
ON eth
```

#### Fetching logs using event signature
```sql
// Using event signature
GET * FROM log WHERE block = 4638757, event_signature = Confirmation(address,uint256) ON eth
```

#### Fetching logs using multiple conditions
```sql
// Query with multiple conditions
GET * FROM logs 
WHERE 
address = 0x123...,
block = 17000000,
topic1 = 0x000000000000000000000000{address}
ON eth
```

## WHERE Clause
Syntax:
```sql
WHERE <[conditions, ]>
```

The where clause is used to filter the results of a query. Each condition is must use the operators described [below](#available-operators), and must be separated by a comma.

**Example**:
```sql
GET * FROM tx WHERE block = latest, value > 0 ON eth
```

The where clause is currently only **available** for **transactions** and **logs** queries, and they work differently for each.

For **transactions** queries, the `WHERE` clause demand user to specify the a block number, list of block numbers, or a block range, which is used to filter the transactions by the block they are included in.
Alongside the block height, users can also filter transactions by any other field using the operators described [below](#available-operators), which will be used to filter the transactions accordingly in memory.

**Example**:
Get all transactions from the latest block with a value greater than 0 ether
```sql
GET * FROM tx WHERE block = latest AND value > 0 ether ON eth
```

For **logs** queries, the `WHERE` clause is used to pass filter parameters to the JSON-RPC method called `eth_getLogs`, which is used to filter the logs by the given parameters, therefore the only supported operator is `=`.
This `WHERE` clause also requires either a block number or a block range to be specified.

**Example**:
Get all logs emitted by the contract in block 4638757 with the event signature `Confirmation(address,uint256)`
```sql
GET * FROM log WHERE block = 4638757, event_signature = Confirmation(address,uint256) ON eth
```

### Available Operators
- `=`: Equal to
- `!=`: Not equal to
- `>`: Greater than
- `<`: Less than
- `>=`: Greater than or equal to
- `<=`: Less than or equal to

## File Exports
Query results can be exported to various file formats using the `>>` operator. The syntax is:

```sql
GET <fields> FROM <entity> [WHERE <conditions>] ON <chains> >> filename.format
```

### Supported Formats
- `json`: JavaScript Object Notation
- `csv`: Comma-Separated Values
- `parquet`: Apache Parquet columnar storage

### Export Examples

#### Exporting account balances to CSV
```sql
GET balance FROM account 0x123...abc ON eth >> balances.csv
```

#### Exporting account balances to JSON
```sql
GET balance FROM account 0x123...abc ON eth >> balances.json
```

#### Exporting account balances to multiple chains
```sql
GET balance FROM account 0x123...abc ON eth, polygon >> multichain_balances.json
```

#### Exporting transaction history to Parquet
```sql
GET * FROM tx WHERE block = 1:100, from = 0x456... >> tx_history.parquet
```

### File Naming
- File names can include alphanumeric characters, hyphens, underscores, and forward slashes
- Forward slashes can be used to specify subdirectories
- File extension must match one of the supported formats

## Limitations
As EQL uses JSON-RPC providers as the backbone for querying, it inherits the same a few limitations, most commonly:
1. Rate limits apply based on RPC provider
2. Some complex queries may timeout on congested networks

For rate limits, EQL enables users to specify their own RPC providers when installed locally. Check out the [installation guide](./installation.md) for more details.
