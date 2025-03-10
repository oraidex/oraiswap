# OraiSwap

Uniswap-inspired automated market-maker (AMM) protocol powered by Smart Contracts on the [Oraichain](https://orai.io) mainnet.

## Contracts

| Name                                                      | Description                                              |
| --------------------------------------------------------- | -------------------------------------------------------- |
| [`oraiswap_factory`](contracts/oraiswap_factory)          | Proxy contract to create oraiswap_pair instance          |
| [`oraiswap_oracle`](contracts/oraiswap_oracle)            | Global parameters updated by multisig wallet             |
| [`oraiswap_pair`](contracts/oraiswap_pair)                | Logic for building liquidity pool and trade between pair |
| [`oraiswap_router`](contracts/oraiswap_router)            | Facilitate multi-hop swap operations                     |
| [`oraiswap_orderbook`](contracts/oraiswap_orderbook)      | Orderbook implementation                                 |
| [`oraiswap_staking`](contracts/oraiswap_staking)          | Stake LPs to get ORAIX reward                            |
| [`oraiswap_token`](contracts/oraiswap_token)              | (ERC20 equivalent) token implementation, AIRI, ORAIX     |
| [`oraiswap_mixed_router`](contracts/oraiswap_mixedrouter) | Facilitate multi-hop swap operations between v2 & v3     |

## Running this contract

You will need Rust 1.44.1+ with wasm32-unknown-unknown target installed.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
```

You can run unit tests on this on each contracts directory via :

```bash
cargo test
```

## Build the contracts

### Prerequisites

- [CosmWasm tools](https://docs.orai.io/developer-guides/cosmwasm-contract/compile-contract#install-cosmwasm-tools)

### Build commands

Build contracts:

```bash
cwtools build contracts/*

# specify the build destination
cwtools build contracts/* -o contract-builds/

# hot-reload auto build after modifying the code
cwtools build contracts/* -w

# build schema. -s is for schema
cwtools build ../tonbridge-cw-contracts/contracts/* -s
# gen typescript codes. -o here is the output directory
cwtools gents ../tonbridge-cw-contracts/contracts/* -o packages/contracts-sdk/src
```

Build schemas:

```bash
cwtools build contracts/* -s
```

Build typescript gen:

```bash
cwtools gents contracts/* -o contract-typescript-gen/
```

## Gen proto definitions

```bash
# gen protobuf response.rs
cargo install protobuf-codegen
protoc --rust_out . response.proto
tee -a response.proto << END
impl ::std::convert::TryFrom<&[u8]> for MsgInstantiateContractResponse {
    type Error = ::protobuf::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
       ::protobuf::Message::parse_from_bytes(value)
    }
}
END

# gen proto using prost
# macos
brew install protobuf

cargo install protoc-gen-prost

protoc --prost_out packages/oraiswap/src --proto_path packages/oraiswap/src -I proto packages/oraiswap/src/universal_swap_memo.proto && mv packages/oraiswap/src/_ packages/oraiswap/src/universal_swap_memo.rs
```
