#![allow(dead_code)]
use cosmwasm_std::{Api, Order, Response, StdResult, Storage};
use oraiswap::asset::AssetInfo;

use crate::{
    legacy::v1::{
        old_read_all_is_migrated, old_read_pool_info, old_read_rewards_per_sec, old_rewards_read,
        old_stakers_read,
    },
    state::{
        read_is_migrated, rewards_store, stakers_store, store_is_migrated, store_pool_info,
        store_rewards_per_sec,
    },
};

pub fn migrate_single_asset_key_to_lp_token(
    storage: &mut dyn Storage,
    api: &dyn Api,
    asset_key: &[u8],
) -> StdResult<u64> {
    let pool_info = old_read_pool_info(storage, asset_key)?;
    // store pool_info to new key
    store_pool_info(storage, &pool_info.staking_token, &pool_info)?;
    let staking_token = api.addr_humanize(&pool_info.staking_token)?;

    if let Ok(native_token) = String::from_utf8(asset_key.to_vec()) {
        #[cfg(debug_assertions)]
        api.debug(&format!(
            "native {}, lp {}",
            native_token.as_str(),
            staking_token.as_str()
        ));
    } else {
        let key = api.addr_humanize(&asset_key.into())?.to_string();

        #[cfg(debug_assertions)]
        api.debug(&format!(
            "cw20 {}, lp {}",
            key.as_str(),
            staking_token.as_str()
        ));
    };
    // store reward_per_sec to new new key
    if let Ok(rewards_per_sec) = old_read_rewards_per_sec(storage, asset_key) {
        #[cfg(debug_assertions)]
        api.debug(&format!("rewards_per_sec {:?}", rewards_per_sec));
        store_rewards_per_sec(storage, &pool_info.staking_token, rewards_per_sec)?;
    }

    let stakers = old_stakers_read(storage, asset_key)
        .range(None, None, Order::Ascending)
        // Get next_key
        .collect::<StdResult<Vec<(Vec<u8>, bool)>>>()?;

    #[cfg(debug_assertions)]
    api.debug(&format!("stakers.len {:?} ", stakers.len()));

    // Store stakers to new staking key token
    for (staker, _) in stakers.iter() {
        let all_is_migrated = old_read_all_is_migrated(storage, staker)?;
        for (old_asset_key, old_is_migrated) in all_is_migrated {
            let old_pool_info = old_read_pool_info(storage, &old_asset_key)?;
            let new_is_migrated = read_is_migrated(storage, &old_pool_info.staking_token, staker);
            if old_is_migrated && !new_is_migrated {
                store_is_migrated(storage, &old_pool_info.staking_token, staker)?;
            }
        }
        stakers_store(storage, &pool_info.staking_token).save(staker, &true)?;
        if let Ok(reward) = old_rewards_read(storage, staker).load(asset_key) {
            rewards_store(storage, staker).save(&pool_info.staking_token, &reward)?;
        }
    }
    Ok(stakers.len() as u64)
}

pub fn migrate_store(
    storage: &mut dyn Storage,
    api: &dyn Api,
    asset_info: AssetInfo,
) -> StdResult<Response> {
    let asset_key = asset_info.to_vec(api)?;

    let total_staker = migrate_single_asset_key_to_lp_token(storage, api, asset_key.as_slice())?;

    Ok(Response::default().add_attributes(vec![
        ("action", "migrate_store"),
        ("asset_info", &asset_info.to_string()),
        ("staker_count", &total_staker.to_string()),
    ]))
}
