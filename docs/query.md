# Query Syntax Guide

## Table of Contents
- [Basic Syntax](#basic-syntax)
- [Query Examples](#query-examples)
  - [Account Queries](#account-queries)
  - [Block Queries](#block-queries)
  - [Transaction Queries](#transaction-queries)
  - [Event Log Queries](#event-log-queries)
- [Field Types](#field-types)
  - [Account Fields](#account-fields)
  - [Block Fields](#block-fields)
  - [Transaction Fields](#transaction-fields)
  - [Log Fields](#log-fields)
- [Operators](#operators)
- [File Exports](#file-exports)
- [Limitations](#limitations)
- [Best Practices](#best-practices)
- [Error Handling](#error-handling)

## Basic Syntax

EQL queries follow this general structure:
```sql
GET <fields> FROM <entity> [WHERE <conditions>] ON <chains>
```

### Components
- `GET`: Specifies the fields you want to retrieve
- `FROM`: Defines the entity type to query
- `WHERE`: (Optional) Filters the results
- `ON`: (Optional) Specifies target chains (defaults to Ethereum mainnet)

### Entity Identifiers
Entities can be queried using:
- Single ID: `GET balance FROM account 0x123... ON eth`
- Multiple IDs: `GET balance FROM account 0x123..., 0x456..., vitalik.eth ON eth`

## Query Examples

### Account Queries

```sql
// Basic balance query
GET balance FROM account 0x123...abc

// Multiple fields
GET balance, nonce FROM account vitalik.eth

// Cross-chain balance check
GET balance FROM account 0x123...abc ON eth, polygon, arbitrum
```

### Block Queries

```sql
// Get latest block
GET number, timestamp FROM block latest ON eth

// Get specific block
GET number, hash, transactions FROM block 17000000 ON eth

// Get block range
GET number, gasUsed FROM block 1:1000
```

### Transaction Queries

```sql
// Get transaction by hash
GET from, to, value FROM transaction 0x123... ON eth

// Get all transactions from an address
GET to, value FROM transaction 0x456... ON eth LIMIT 100

// Filter transactions from blocks
GET * FROM transaction WHERE block = latest, value > 0 ON eth
```

### Event Log Queries

```sql
// Query ERC20 transfer events
GET * FROM logs 
WHERE 
address = '0x123...',
topic0 = '0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef'
ON eth

// Query with multiple conditions
GET * FROM logs 
WHERE address = '0x123...' 
AND blockNumber > 17000000 
AND topic1 = '0x000000000000000000000000{address}'
ON eth
```

## Field Types

### Account Fields
- `balance`: Current balance in wei
- `nonce`: Transaction count
- `code`: Contract bytecode (if contract account)
- `address`: Account address

### Block Fields
- `number`: Block number
- `hash`: Block hash
- `timestamp`: Block timestamp
- `transactions`: Array of transaction hashes
- `gasUsed`: Gas used in the block
- `gasLimit`: Block gas limit

### Transaction Fields
- `hash`: Transaction hash
- `from`: Sender address
- `to`: Recipient address
- `value`: Transaction value in wei
- `input`: Transaction input data
- `nonce`: Transaction nonce
- `gasPrice`: Gas price in wei
- `gasLimit`: Gas limit

### Log Fields
- `address`: Contract address
- `topics`: Array of topics
- `data`: Log data
- `blockNumber`: Block number
- `transactionHash`: Parent transaction hash

## Operators

### Comparison Operators
- `=`: Equal to
- `!=`: Not equal to
- `>`: Greater than
- `<`: Less than
- `>=`: Greater than or equal to
- `<=`: Less than or equal to

### Logical Operators
- `AND`: Logical AND
- `OR`: Logical OR
- `NOT`: Logical NOT

### Special Values
- `latest`: Latest block
- `pending`: Pending block
- `earliest`: Genesis block
- `ether`: Multiplier for wei values (1 ether = 1e18 wei)
- `gwei`: Multiplier for wei values (1 gwei = 1e9 wei)

## File Exports

Query results can be exported to various file formats using the `>>` operator. The syntax is:

```sql
GET <fields> FROM <entity> [WHERE <conditions>] ON <chains> >> filename.format
```

### Supported Formats
- `json`: JavaScript Object Notation
- `csv`: Comma-Separated Values
- `yaml`: YAML Ain't Markup Language
- `toml`: Tom's Obvious Minimal Language
- `parquet`: Apache Parquet columnar storage

### Export Examples

```sql
// Export account balances to CSV
GET balance FROM account 0x123...abc ON eth >> balances.csv

// Export multiple chain data to JSON
GET balance FROM account 0x123...abc ON eth,polygon >> multichain_balances.json

// Export transaction history to Parquet
GET * FROM transaction WHERE from = '0x456...' >> tx_history.parquet
```

### File Naming
- File names can include alphanumeric characters, hyphens, underscores, and forward slashes
- Forward slashes can be used to specify subdirectories
- File extension must match one of the supported formats

## Limitations

- Rate limits apply based on RPC provider
- Some complex queries may timeout on congested networks

## Best Practices

1. **Use Specific Fields**
   - Request only needed fields instead of `*`
   - Reduces response size and processing time

2. **Add Block Range Limits**
   - Always specify block ranges for historical queries
   - Helps prevent timeouts on large ranges

3. **Handle ENS Names**
   - ENS names are automatically resolved
   - Cache resolutions for frequently used addresses

4. **Optimize Log Queries**
   - Always include address and at least one topic
   - Use specific block ranges for better performance

## Error Handling

Common error messages and their solutions:

```sql
ERROR: Invalid address format
SOLUTION: Ensure address is valid hex or ENS name

ERROR: Block range too large
SOLUTION: Reduce block range to <= 100 blocks

ERROR: Rate limit exceeded
SOLUTION: Add delay between queries or reduce query frequency
```