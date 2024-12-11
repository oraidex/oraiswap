# OraiSwap

Uniswap-inspired automated market-maker (AMM) protocol powered by Smart Contracts on the [Orai](https://orai.io) blockchain.

## Contracts

| Name                                                 | Description                                              |
| ---------------------------------------------------- | -------------------------------------------------------- |
| [`oraiswap_factory`](contracts/oraiswap_factory)     | Proxy contract to create oraiswap_pair instance          |
| [`oraiswap_oracle`](contracts/oraiswap_oracle)       | Global parameters updated by multisig wallet             |
| [`oraiswap_pair`](contracts/oraiswap_pair)           | Logic for building liquidity pool and trade between pair |
| [`oraiswap_router`](contracts/oraiswap_router)       | Facilitate multi-hop swap operations                     |
| [`oraiswap_orderbook`](contracts/oraiswap_orderbook) | Orderbook implementation                                 |
| [`oraiswap_staking`](contracts/oraiswap_staking)     | Stake LPs to get ORAIX reward                            |
| [`oraiswap_token`](contracts/oraiswap_token)         | (ERC20 equivalent) token implementation, AIRI, ORAIX     |
| [`oraiswap_mixed_router`](contracts/oraiswap_mixedrouter)         | Facilitate multi-hop swap operations between v2 & v3     |

- oraiswap_factory

  Mainnet: [`orai167r4ut7avvgpp3rlzksz6vw5spmykluzagvmj3ht845fjschwugqjsqhst`](https://scan.orai.io/smart-contract/orai167r4ut7avvgpp3rlzksz6vw5spmykluzagvmj3ht845fjschwugqjsqhst)

- oraiswap_oracle

  Mainnet: [`orai18rgtdvlrev60plvucw2rz8nmj8pau9gst4q07m`](https://scan.orai.io/smart-contract/orai18rgtdvlrev60plvucw2rz8nmj8pau9gst4q07m)

- oraiswap_pair

  Mainnet (CodeID): 1502

- oraiswap_route

  Mainnet: [`orai1j0r67r9k8t34pnhy00x3ftuxuwg0r6r4p8p6rrc8az0ednzr8y9s3sj2sf`](https://scan.orai.io/smart-contract/orai1j0r67r9k8t34pnhy00x3ftuxuwg0r6r4p8p6rrc8az0ednzr8y9s3sj2sf)

- oraiswap_orderbook

  Mainnet: [`orai1nt58gcu4e63v7k55phnr3gaym9tvk3q4apqzqccjuwppgjuyjy6sxk8yzp`](https://scan.orai.io/smart-contract/orai1nt58gcu4e63v7k55phnr3gaym9tvk3q4apqzqccjuwppgjuyjy6sxk8yzp)

- oraiswap_staking

  Mainnet: [`orai19p43y0tqnr5qlhfwnxft2u5unph5yn60y7tuvu`](https://scan.orai.io/smart-contract/orai19p43y0tqnr5qlhfwnxft2u5unph5yn60y7tuvu)

- oraiswap_token

  Mainnet (CodeID): 582

- oraiswap_mixed_router

  Mainnet: [`orai1cy2pc5czxm5qlacp6j0hfq7qj9wh8zuhxgpdartcfrdljknq0arsuc4znj`](https://scan.orai.io/smart-contract/orai1cy2pc5czxm5qlacp6j0hfq7qj9wh8zuhxgpdartcfrdljknq0arsuc4znj)

## Running this contract

You will need Rust 1.44.1+ with wasm32-unknown-unknown target installed.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
```

You can run unit tests on this on each contracts directory via :

```bash
cargo unit-test
cargo integration-test
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
