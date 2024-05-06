## README for Gavel

### Overview

Gavel is a command-line interface (CLI) tool designed to facilitate the
interaction with blockchain data via WebSockets. It offers a straightforward
and robust method to fetch block data or generate Merkle Mountain Range (MMR)
proofs directly from the blockchain. This tool is particularly useful for
monitoring and verifying GeoDNS endpoints blockchain data.

### Features

- **Fetch Block Data:** Gavel can retrieve specific block data from a blockchain
node using the WebSocket protocol. Users can specify a block number in either
decimal or hexadecimal format or opt to retrieve the latest block by default.
  
- **Generate MMR Proofs:** The tool also supports the generation of MMR proofs
for specified block numbers. If no block number is provided, it will automatically
generate a proof for the latest block.

- **Custom DNS Resolution:** Gavel allows for the manual specification of an IPv4
address for the endpoint, bypassing the DNS resolution which can be critical
for environments with strict networking rules.

### Installation

To install Gavel, clone the repository and build the project using Cargo,
Rust's package manager and build system:

```bash
git clone https://github.com/ibp-network/gavel.git
cd gavel
cargo build --release
cp ./target/release/gavel /usr/local/bin/
chmod +x /usr/local/bin/gavel
```

The executable will be located in `./target/release/`.

### Usage

#### Fetch Command

```bash
gavel fetch [OPTIONS] <ENDPOINT> [BLOCK_NUMBER]
```

- **ENDPOINT**: Specify the WebSocket URL of the blockchain.
- **BLOCK_NUMBER**: Optional. Specify the block number in decimal or hex format
(e.g., `0x1A3B`). If omitted, the latest block is fetched.

Options:
- `-r, --resolve <RESOLVE>`: Manually specify an IPv4 address to resolve the endpoint.

#### MMR Command

```bash
gavel mmr [OPTIONS] <ENDPOINT> [BLOCK_NUMBERS]...
```

- **ENDPOINT**: The WebSocket endpoint URL for the blockchain.
- **BLOCK_NUMBERS**: Optional. A comma-separated list of block numbers.
If omitted, generates a proof for the latest block.

Options:
- `-r, --resolve <RESOLVE>`: Manually specify an IPv4 address to resolve the endpoint.
