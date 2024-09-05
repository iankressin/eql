## Alpha roadmap
![roadmap image](./roadmap.png)


### v0.1.3-alpha
- [x] Logs and filters:
 `GET topic0, topic1 FROM log WHERE block latest address 0x0 ON eth`
- [ ] Dump query results to json, parquet, and, csv:
 `GET nonce, balance FROM vitalik.eth ON eth > data.csv`
- [ ] List of entity ids:
 `GET value, to, timestamp FROM tx 0x..., 0x..., 0x... ON polygon`
- [x] Add support for more EVM chains

### v0.1.4-alpha
- [ ] Get transactions from blocks:
`GET from, to FROM transaction WHERE tx.value 1, block 1:10 ON eth`
- [ ] Wildcard operator for both fields and chains:
 `GET * FROM account vitalik.eth ON *`
- [ ] REPL improvements: Save query history, fix minor bugs

### v0.1.5-alpha
- [ ] User configurable RPC list
- [ ] Support to transaction receipt fields under transaction entity
- [ ] Smart-contract support:
 `GET balanceOf(0x...) FROM contract 0x... ON polygon`

### v0.1.6-alpha
- [ ] Add, div, sub, mul:
`ADD(GET nonce FROM account vitalik.eth ON eth, polygon, base)`
- [ ] Get account balance, nonce at specific block and range:
 `GET nonce, balance FROM vitalik.eth WHERE block 1:10 ON base`
- [ ] Support for custom chains:
 `GET nonce, balance FROM vitalik.eth WHERE block 1:10 ON custom-1`

### v0.1.7-alpha
- [ ] Codebase refactor
