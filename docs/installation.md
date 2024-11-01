# Installation

```shell
curl -L https://raw.githubusercontent.com/iankressin/eql/main/eqlup/install.sh | bash && eqlup
```

This command will:
1. Install `eqlup` (the EQL version manager)
2. Run `eqlup` to install the latest version of EQL

The installation process will:
1. Detect your operating system (Linux/MacOS)
2. Download the appropriate binary
3. Install it to `/usr/local/bin/eql`
4. Create a default configuration file at `~/eql-config.json` with pre-configured RPC endpoints for various networks

## Configuration

The `~/eql-config.json` file contains RPC endpoints for different blockchain networks. You can customize it by adding new chains or modifying existing ones:

```json
{
    "chains": {
        "eth": {
            "default": "https://ethereum.drpc.org",
            "rpcs": [
                "https://ethereum.drpc.org",
                "https://eth.llamarpc.com",
                "https://rpc.ankr.com/eth"
            ]
        },
        "sepolia": {
            "default": "https://rpc.ankr.com/eth_sepolia",
            "rpcs": [
                "https://rpc.ankr.com/eth_sepolia"
            ]
        },
        "bnb": {
            "default": "https://bsc.drpc.org",
            "rpcs": [
                "https://bsc.drpc.org",
                "https://rpc.ankr.com/bsc"
            ]
        },
        "arb": {
            "default": "https://rpc.ankr.com/arbitrum",
            "rpcs": [
                "https://rpc.ankr.com/arbitrum"
            ]
        },
        "base": {
            "default": "https://rpc.ankr.com/base",
            "rpcs": [
                "https://rpc.ankr.com/base"
            ]
        },
        "blast": {
            "default": "https://rpc.ankr.com/blast",
            "rpcs": [
                "https://rpc.ankr.com/blast"
            ]
        },
        "op": {
            "default": "https://rpc.ankr.com/optimism",
            "rpcs": [
                "https://rpc.ankr.com/optimism"
            ]
        },
        "polygon": {
            "default": "https://polygon.llamarpc.com",
            "rpcs": [
                "https://polygon.llamarpc.com"
            ]
        },
        "anvil": {
            "default": "http://localhost:8545",
            "rpcs": [
                "http://localhost:8545"
            ]
        },
        "mantle": {
            "default": "https://mantle.drpc.org",
            "rpcs": [
                "https://mantle.drpc.org"
            ]
        },
        "zksync": {
            "default": "https://zksync.drpc.org",
            "rpcs": [
                "https://zksync.drpc.org"
            ]
        },
        "taiko": {
            "default": "https://rpc.taiko.xyz",
            "rpcs": [
                "https://rpc.taiko.xyz"
            ]
        },
        "celo": {
            "default": "https://forno.celo.org",
            "rpcs": [
                "https://forno.celo.org"
            ]
        },
        "avalanche": {
            "default": "https://avalanche.drpc.org",
            "rpcs": [
                "https://avalanche.drpc.org"
            ]
        },
        "scroll": {
            "default": "https://scroll.drpc.org",
            "rpcs": [
                "https://scroll.drpc.org"
            ]
        },
        "linea": {
            "default": "https://rpc.linea.build",
            "rpcs": [
                "https://rpc.linea.build"
            ]
        },
        "zora": {
            "default": "https://zora.drpc.org",
            "rpcs": [
                "https://zora.drpc.org"
            ]
        },
        "moonbeam": {
            "default": "https://moonbeam.drpc.org",
            "rpcs": [
                "https://moonbeam.drpc.org"
            ]
        },
        "moonriver": {
            "default": "https://moonriver.drpc.org",
            "rpcs": [
                "https://moonriver.drpc.org"
            ]
        },
        "ronin": {
            "default": "https://ronin.drpc.org",
            "rpcs": [
                "https://ronin.drpc.org"
            ]
        },
        "fantom": {
            "default": "https://fantom.drpc.org",
            "rpcs": [
                "https://fantom.drpc.org"
            ]
        },
        "kava": {
            "default": "https://kava.drpc.org",
            "rpcs": [
                "https://kava.drpc.org"
            ]
        },
        "gnosis": {
            "default": "https://gnosis.drpc.org",
            "rpcs": [
                "https://gnosis.drpc.org"
            ]
        }
    }
}
```

### Pre-configured Networks

The default configuration includes popular networks like Ethereum, BNB Chain, Arbitrum, and many others. You can find the complete list here:
- Ethereum
- Sepolia
- BNB Chain
- Arbitrum
- Base
- Optimism
- Polygon
- Avalanche
- Scroll
- Linea
- Zora
- Moonbeam
- Moonriver
- Ronin
- Fantom
- Kava
- Gnosis

## Verify Installation

After installation, verify that everything is working:
```shell
eql --help
```