use cosmwasm_schema::cw_serde;

use cosmwasm_std::{Addr, Api, CanonicalAddr, Order, StdResult, Storage};
use cw_storage_plus::{Bound, Item, Map};
use oraiswap::asset::{AssetInfoRaw, PairInfo, PairInfoRaw};

#[cw_serde]
pub struct Config {
    pub owner: CanonicalAddr,
    pub oracle_addr: CanonicalAddr,
    pub pair_code_id: u64,
    pub token_code_id: u64,
    pub commission_rate: String,
    pub operator_fee: String,
    pub operator: CanonicalAddr,
}

#[cw_serde]
pub struct RestrictedAssets {
    // token factory contract address (token-bindings), only store the prefix "factory/orai1"
    pub assets: Vec<String>,
}

#[cw_serde]
pub struct Creator {
    pub whitelist_addresses: Vec<Addr>,
}

// put the length bytes at the first for compatibility with legacy singleton store
pub const CONFIG: Item<Config> = Item::new("\u{0}\u{6}config");

// store temporary pair info while waiting for deployment
pub const PAIRS: Map<&[u8], PairInfoRaw> = Map::new("pairs");

pub const RESTRICTED_ASSETS: Item<RestrictedAssets> = Item::new("restricted_assets");
pub const CREATOR: Item<Creator> = Item::new("creator");

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_pairs(
    storage: &dyn Storage,
    api: &dyn Api,
    start_after: Option<[AssetInfoRaw; 2]>,
    limit: Option<u32>,
) -> StdResult<Vec<PairInfo>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = calc_range_start(start_after).map(Bound::ExclusiveRaw);

    PAIRS
        .range(storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (_, v) = item?;
            v.to_normal(api)
        })
        .collect::<StdResult<Vec<PairInfo>>>()
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_start(start_after: Option<[AssetInfoRaw; 2]>) -> Option<Vec<u8>> {
    start_after.map(|asset_infos| {
        let mut asset_infos = asset_infos.to_vec();
        asset_infos.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let mut v = [asset_infos[0].as_bytes(), asset_infos[1].as_bytes()]
            .concat()
            .as_slice()
            .to_vec();
        v.push(1);
        v
    })
}

#[cfg(test)]
mod test {
    use super::*;

    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{Api, StdResult, Storage};
    use cosmwasm_storage::{
        bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket,
    };
    use oraiswap::asset::pair_key;
    use oraiswap::pair::{DEFAULT_COMMISSION_RATE, DEFAULT_OPERATOR_FEE};
    const KEY_CONFIG: &[u8] = b"config";

    pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
        singleton(storage, KEY_CONFIG).save(config)
    }
    pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
        singleton_read(storage, KEY_CONFIG).load()
    }

    #[test]
    fn config_legacy_compatibility() {
        let mut deps = mock_dependencies();
        store_config(
            &mut deps.storage,
            &Config {
                oracle_addr: deps.api.addr_canonicalize("oracle0000").unwrap(),
                owner: deps.api.addr_canonicalize("owner0000").unwrap(),
                pair_code_id: 1,
                token_code_id: 1,
                commission_rate: DEFAULT_COMMISSION_RATE.to_string(),
                operator_fee: DEFAULT_OPERATOR_FEE.to_string(),
                operator: deps.api.addr_canonicalize("operator").unwrap(),
            },
        )
        .unwrap();

        assert_eq!(
            CONFIG.load(&deps.storage).unwrap(),
            read_config(&deps.storage).unwrap()
        );
    }

    const PREFIX_PAIRS: &[u8] = b"pairs";
    pub fn store_pair(storage: &mut dyn Storage, data: &PairInfoRaw) -> StdResult<()> {
        let mut asset_infos = data.asset_infos.clone().to_vec();
        asset_infos.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let mut pair_bucket: Bucket<PairInfoRaw> = bucket(storage, PREFIX_PAIRS);
        pair_bucket.save(
            &[asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat(),
            data,
        )
    }
    pub fn read_pair(
        storage: &dyn Storage,
        asset_infos: &[AssetInfoRaw; 2],
    ) -> StdResult<PairInfoRaw> {
        let mut asset_infos = asset_infos.clone().to_vec();
        asset_infos.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let pair_bucket: ReadonlyBucket<PairInfoRaw> = bucket_read(storage, PREFIX_PAIRS);
        pair_bucket.load(&[asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat())
    }

    pub fn legacy_read_pairs(
        storage: &dyn Storage,
        api: &dyn Api,
        start_after: Option<[AssetInfoRaw; 2]>,
        limit: Option<u32>,
    ) -> StdResult<Vec<PairInfo>> {
        let pair_bucket: ReadonlyBucket<PairInfoRaw> = bucket_read(storage, PREFIX_PAIRS);
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start = calc_range_start(start_after);
        pair_bucket
            .range(start.as_deref(), None, Order::Ascending)
            .take(limit)
            .map(|item| {
                let (_, v) = item?;
                v.to_normal(api)
            })
            .collect()
    }

    #[test]
    fn pair_info_legacy_compatibility() {
        let mut deps = mock_dependencies();
        let pair_info = PairInfoRaw {
            oracle_addr: deps.api.addr_canonicalize("oracle0000").unwrap(),
            asset_infos: [
                AssetInfoRaw::NativeToken {
                    denom: "uusd".to_string(),
                },
                AssetInfoRaw::Token {
                    contract_addr: deps.api.addr_canonicalize("token0000").unwrap(),
                },
            ],
            contract_addr: deps.api.addr_canonicalize("pair0000").unwrap(),
            liquidity_token: deps.api.addr_canonicalize("liquidity0000").unwrap(),
            commission_rate: DEFAULT_COMMISSION_RATE.to_string(),
            operator_fee: DEFAULT_OPERATOR_FEE.to_string(),
        };

        let pair_info2 = PairInfoRaw {
            oracle_addr: deps.api.addr_canonicalize("oracle0000").unwrap(),
            asset_infos: [
                AssetInfoRaw::NativeToken {
                    denom: "uusd".to_string(),
                },
                AssetInfoRaw::Token {
                    contract_addr: deps.api.addr_canonicalize("token0001").unwrap(),
                },
            ],
            contract_addr: deps.api.addr_canonicalize("pair0001").unwrap(),
            liquidity_token: deps.api.addr_canonicalize("liquidity0001").unwrap(),
            commission_rate: DEFAULT_COMMISSION_RATE.to_string(),
            operator_fee: DEFAULT_OPERATOR_FEE.to_string(),
        };

        store_pair(&mut deps.storage, &pair_info).unwrap();
        store_pair(&mut deps.storage, &pair_info2).unwrap();

        assert_eq!(
            PAIRS
                .load(&deps.storage, &pair_key(&pair_info.asset_infos))
                .unwrap(),
            read_pair(&deps.storage, &pair_info.asset_infos).unwrap()
        );

        assert_eq!(
            PAIRS
                .load(&deps.storage, &pair_key(&pair_info2.asset_infos))
                .unwrap(),
            read_pair(&deps.storage, &pair_info2.asset_infos).unwrap()
        );

        assert_eq!(
            read_pairs(&deps.storage, &deps.api, None, None),
            legacy_read_pairs(&deps.storage, &deps.api, None, None),
        );
    }
}
