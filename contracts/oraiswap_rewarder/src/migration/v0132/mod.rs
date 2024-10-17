use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Api, CanonicalAddr, Order, StdResult, Storage};
use cosmwasm_storage::{singleton, singleton_read, Bucket, ReadonlyBucket};
use oraiswap::rewarder::MigrateMsg;

use crate::state::{store_config, store_last_distributed, Config as Config15};

static KEY_CONFIG: &[u8] = b"config";
static KEY_LAST_DISTRIBUTED: &[u8] = b"last_distributed";

#[cw_serde]
pub struct Config {
    pub owner: CanonicalAddr,
    pub staking_contract: CanonicalAddr,
    pub distribution_interval: u64,
    pub init_time: u64,
}

pub fn store_config_0132(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config_0132(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

pub fn read_last_distributed_0132(storage: &dyn Storage, asset_key: &[u8]) -> StdResult<u64> {
    ReadonlyBucket::new(storage, KEY_LAST_DISTRIBUTED).load(asset_key)
}

pub fn store_last_distributed_0132(
    storage: &mut dyn Storage,
    asset_key: &[u8],
    last_distributed: u64,
) -> StdResult<()> {
    Bucket::new(storage, KEY_LAST_DISTRIBUTED).save(asset_key, &last_distributed)
}

pub fn iterate_all_last_distributed_0132(
    storage: &mut dyn Storage,
) -> StdResult<Vec<(Vec<u8>, u64)>> {
    let bucket: Bucket<'_, u64> = Bucket::new(storage, KEY_LAST_DISTRIBUTED);
    let all_distributed: Vec<(Vec<u8>, u64)> = bucket
        .range(None, None, Order::Ascending)
        .filter_map(|item_result| -> Option<(Vec<u8>, u64)> { item_result.ok() })
        .collect::<Vec<_>>();
    Ok(all_distributed)
}

pub fn migrate_0132_to_15(
    storage: &mut dyn Storage,
    api: &dyn Api,
    msg: MigrateMsg,
    init_time: u64,
) -> StdResult<()> {
    // migrate config
    store_config(
        storage,
        &Config15 {
            owner: api.addr_canonicalize(&msg.owner)?,
            staking_contract: api.addr_canonicalize(&msg.staking_contract)?,
            distribution_interval: msg.distribution_interval,
            init_time,
        },
    )?;

    // migrate all last distributed values
    let all_distributed = iterate_all_last_distributed_0132(storage)?;
    for distributed in all_distributed {
        store_last_distributed(storage, &distributed.0, distributed.1)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use cosmwasm_std::{testing::mock_dependencies, CanonicalAddr};
    use oraiswap::rewarder::MigrateMsg;

    use crate::{
        migration::v0132::migrate_0132_to_15,
        state::{read_config, read_last_distributed, Config},
    };

    use super::{store_config_0132, store_last_distributed_0132, Config as Config0132};

    #[test]
    fn test_migrate_store_0132_to_15() {
        // fixture
        let mut deps = mock_dependencies();
        let deps_mut = deps.as_mut();
        let owner_canon = CanonicalAddr::from(&[1,2,3]);
        let new_owner_canon = CanonicalAddr::from(&[2,3,4]);
        let staking_canon = CanonicalAddr::from(&[3,4,5]);
        let new_staking_canon = CanonicalAddr::from(&[4,5,6]);

        store_config_0132(
            deps_mut.storage,
            &Config0132 {
                owner: owner_canon,
                staking_contract: staking_canon,
                distribution_interval: 1,
                init_time: 1,
            },
        )
        .unwrap();

        store_last_distributed_0132(deps_mut.storage, &[1], 10).unwrap();
        store_last_distributed_0132(deps_mut.storage, &[2], 100).unwrap();
        store_last_distributed_0132(deps_mut.storage, &[10], 500).unwrap();

        // now the fun part, migrate
        migrate_0132_to_15(
            deps_mut.storage,
            deps_mut.api,
            MigrateMsg {
                owner: new_owner_canon.to_string(),
                staking_contract: new_staking_canon.to_string(),
                distribution_interval: 99,
            },
            1,
        )
        .unwrap();

        // assertion. new stores should have matching data
        let config = read_config(deps_mut.storage).unwrap();

        assert_eq!(
            config,
            Config {
                owner: deps_mut.api.addr_canonicalize(&new_owner_canon.to_string()).unwrap(),
                staking_contract: deps_mut.api.addr_canonicalize(&new_staking_canon.to_string()).unwrap(),
                distribution_interval: 99,
                init_time: 1,
            }
        );
        let last_distributed = read_last_distributed(deps_mut.storage, &[1]).unwrap();
        assert_eq!(last_distributed, 10);
        let last_distributed = read_last_distributed(deps_mut.storage, &[2]).unwrap();
        assert_eq!(last_distributed, 100);
        let last_distributed = read_last_distributed(deps_mut.storage, &[10]).unwrap();
        assert_eq!(last_distributed, 500);
    }
}
