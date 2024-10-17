use cosmwasm_schema::cw_serde;
use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
use cosmwasm_storage::{singleton, singleton_read, Bucket, ReadonlyBucket};

static KEY_CONFIG: &[u8] = b"config_std_1_5";
static KEY_LAST_DISTRIBUTED: &[u8] = b"last_distributed_std_1_5";

#[cw_serde]
pub struct Config {
    pub owner: CanonicalAddr,
    pub staking_contract: CanonicalAddr,
    pub distribution_interval: u64,
    pub init_time: u64,
}

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

pub fn store_last_distributed(
    storage: &mut dyn Storage,
    asset_key: &[u8],
    last_distributed: u64,
) -> StdResult<()> {
    Bucket::new(storage, KEY_LAST_DISTRIBUTED).save(asset_key, &last_distributed)
}

pub fn read_last_distributed(storage: &dyn Storage, asset_key: &[u8]) -> StdResult<u64> {
    ReadonlyBucket::new(storage, KEY_LAST_DISTRIBUTED).load(asset_key)
}
