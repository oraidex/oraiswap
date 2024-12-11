use crate::contract::validate_migrate_store_status;
use crate::rewards::before_share_change;
use crate::state::{
    read_config, read_is_migrated, read_pool_info, rewards_read, rewards_store, stakers_store,
    store_is_migrated, store_pool_info, Config, PoolInfo, RewardInfo,
};
use cosmwasm_std::{
    attr, to_json_binary, Addr, Api, CanonicalAddr, Coin, CosmosMsg, Decimal, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use oraiswap::asset::{Asset, AssetInfo, PairInfo};
use oraiswap::pair::ExecuteMsg as PairExecuteMsg;
use oraiswap::querier::{query_pair_info, query_token_balance};
use oraiswap::staking::ExecuteMsg;

pub fn bond(
    deps: DepsMut,
    staker_addr: Addr,
    staking_token: Addr,
    amount: Uint128,
) -> StdResult<Response> {
    let staker_addr_raw: CanonicalAddr = deps.api.addr_canonicalize(staker_addr.as_str())?;
    _increase_bond_amount(
        deps.storage,
        deps.api,
        &staker_addr_raw,
        staking_token.clone(),
        amount,
    )?;

    Ok(Response::new().add_attributes([
        ("action", "bond"),
        ("staker_addr", staker_addr.as_str()),
        ("staking_token", staking_token.as_str()),
        ("amount", &amount.to_string()),
    ]))
}

pub fn unbond(
    deps: DepsMut,
    _env: Env,
    staker_addr: Addr,
    staking_token: Addr,
    amount: Uint128,
) -> StdResult<Response> {
    validate_migrate_store_status(deps.storage)?;
    let staker_addr_raw: CanonicalAddr = deps.api.addr_canonicalize(staker_addr.as_str())?;
    let (staking_token, reward_assets) = _decrease_bond_amount(
        deps.storage,
        deps.api,
        &staker_addr_raw,
        &staking_token,
        amount,
    )?;

    let staking_token_addr = deps.api.addr_humanize(&staking_token)?;
    let mut messages = vec![WasmMsg::Execute {
        contract_addr: staking_token_addr.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
            recipient: staker_addr.to_string(),
            amount,
        })?,
        funds: vec![],
    }
    .into()];

    // withdraw pending_withdraw assets (accumulated when changing reward_per_sec)
    messages.extend(
        reward_assets
            .into_iter()
            .map(|ra| ra.into_msg(None, &deps.querier, staker_addr.clone()))
            .collect::<StdResult<Vec<CosmosMsg>>>()?,
    );

    Ok(Response::new().add_messages(messages).add_attributes([
        attr("action", "unbond"),
        attr("staker_addr", staker_addr.as_str()),
        attr("amount", amount.to_string()),
        attr("staking_token", staking_token_addr.as_str()),
    ]))
}

pub fn auto_stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: [Asset; 2],
    slippage_tolerance: Option<Decimal>,
) -> StdResult<Response> {
    validate_migrate_store_status(deps.storage)?;
    let config: Config = read_config(deps.storage)?;
    let factory_addr = deps.api.addr_humanize(&config.factory_addr)?;

    // query pair info to obtain pair contract address
    let asset_infos: [AssetInfo; 2] = [assets[0].info.clone(), assets[1].info.clone()];
    let oraiswap_pair: PairInfo = query_pair_info(&deps.querier, factory_addr, &asset_infos)?;

    let staking_token = deps
        .api
        .addr_canonicalize(oraiswap_pair.liquidity_token.as_str())?;
    let asset_key = staking_token.as_slice();

    // assert the token and lp token match with pool info
    let pool_info = read_pool_info(deps.storage, asset_key)?;

    if pool_info.staking_token != staking_token {
        return Err(StdError::generic_err("Invalid staking token"));
    }

    // get current lp token amount to later compute the recived amount
    let prev_staking_token_amount = query_token_balance(
        &deps.querier,
        oraiswap_pair.liquidity_token.clone(),
        env.contract.address.clone(),
    )?;

    let mut msgs = vec![];
    let mut funds = vec![];

    for asset in assets.iter() {
        match asset.info.clone() {
            AssetInfo::NativeToken { .. } => {
                asset.assert_sent_native_token_balance(&info)?;
                funds.push(Coin {
                    denom: asset.info.to_string(),
                    amount: asset.amount,
                });
            }
            AssetInfo::Token { contract_addr } => {
                // 1. Transfer token asset to staking contract
                // 2. Increase allowance of token for pair contract
                // require transfer and increase allowance
                msgs.push(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount: asset.amount,
                    })?,
                    funds: vec![],
                });
                msgs.push(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                        spender: oraiswap_pair.contract_addr.to_string(),
                        amount: asset.amount,
                        expires: None,
                    })?,
                    funds: vec![],
                });
            }
        }
    }

    // 3. Provide liquidity
    // provide liquidity with funds from native tokens, run first
    msgs.push(WasmMsg::Execute {
        contract_addr: oraiswap_pair.contract_addr.to_string(),
        msg: to_json_binary(&PairExecuteMsg::ProvideLiquidity {
            assets: assets.clone(),
            slippage_tolerance,
            receiver: None,
        })?,
        funds,
    });

    // 4. Execute staking hook, will stake in the name of the sender
    // then auto stake hoook
    msgs.push(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&ExecuteMsg::AutoStakeHook {
            staking_token: oraiswap_pair.liquidity_token.clone(),
            staker_addr: info.sender,
            prev_staking_token_amount,
        })?,
        funds: vec![],
    });

    Ok(Response::new().add_messages(msgs).add_attributes([
        ("action", "auto_stake"),
        ("staking_token", oraiswap_pair.liquidity_token.as_str()),
    ]))
}

pub fn auto_stake_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    staking_token: Addr,
    staker_addr: Addr,
    prev_staking_token_amount: Uint128,
) -> StdResult<Response> {
    // only can be called by itself
    validate_migrate_store_status(deps.storage)?;
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    // stake all lp tokens received, compare with staking token amount before liquidity provision was executed
    let current_staking_token_amount =
        query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;
    let amount_to_stake = current_staking_token_amount.checked_sub(prev_staking_token_amount)?;

    bond(deps, staker_addr, staking_token, amount_to_stake)
}

fn _increase_bond_amount(
    storage: &mut dyn Storage,
    api: &dyn Api,
    staker_addr: &CanonicalAddr,
    staking_token: Addr,
    amount: Uint128,
) -> StdResult<()> {
    let asset_key = api.addr_canonicalize(staking_token.as_str())?.to_vec();
    let mut pool_info = read_pool_info(storage, &asset_key)?;
    let mut reward_info: RewardInfo = rewards_read(storage, staker_addr)
        .load(&asset_key)
        .unwrap_or_else(|_| RewardInfo {
            native_token: false,
            index: Decimal::zero(),
            bond_amount: Uint128::zero(),
            pending_reward: Uint128::zero(),
            pending_withdraw: vec![],
        });

    // check if the position should be migrated
    let is_position_migrated = read_is_migrated(storage, &asset_key, staker_addr);
    if pool_info.migration_params.is_some() {
        // the pool has been migrated, if position is not migrated and has tokens bonded, return error
        if !reward_info.bond_amount.is_zero() && !is_position_migrated {
            return Err(StdError::generic_err("The LP token for this asset has been deprecated, withdraw all your deprecated tokens to migrate your position"));
        } else if !is_position_migrated {
            // if the position is not migrated, but bond amount is zero, it means it's a new position, so store it as migrated
            store_is_migrated(storage, &asset_key, staker_addr)?;
        }
    }

    // Withdraw reward to pending reward; before changing share
    before_share_change(pool_info.reward_index, &mut reward_info)?;

    // Increase total bond amount
    pool_info.total_bond_amount += amount;

    reward_info.bond_amount += amount;

    rewards_store(storage, staker_addr).save(&asset_key, &reward_info)?;
    store_pool_info(storage, &asset_key, &pool_info)?;

    // mark this staker belong to the pool the first time
    let mut stakers_bucket = stakers_store(storage, &asset_key);
    if stakers_bucket.may_load(staker_addr)?.is_none() {
        stakers_bucket.save(staker_addr, &true)?;
    }

    Ok(())
}

fn _decrease_bond_amount(
    storage: &mut dyn Storage,
    api: &dyn Api,
    staker_addr: &CanonicalAddr,
    staking_token: &Addr,
    amount: Uint128,
) -> StdResult<(CanonicalAddr, Vec<Asset>)> {
    let asset_key = api.addr_canonicalize(staking_token.as_str())?.to_vec();
    let mut pool_info: PoolInfo = read_pool_info(storage, &asset_key)?;
    let mut reward_info: RewardInfo = rewards_read(storage, staker_addr).load(&asset_key)?;
    let mut reward_assets = vec![];
    if reward_info.bond_amount < amount {
        return Err(StdError::generic_err("Cannot unbond more than bond amount"));
    }

    // if the lp token was migrated, and the user did not close their position yet, cap the reward at the snapshot
    let should_migrate =
        !read_is_migrated(storage, &asset_key, staker_addr) && pool_info.migration_params.is_some();
    let (pool_index, staking_token) = if should_migrate {
        let migraton_params = pool_info.migration_params.clone().unwrap();
        (
            migraton_params.index_snapshot,
            migraton_params.deprecated_staking_token,
        )
    } else {
        (pool_info.reward_index, pool_info.staking_token.clone())
    };

    // Distribute reward to pending reward; before changing share
    before_share_change(pool_index, &mut reward_info)?;

    // Decrease total bond amount
    if !should_migrate {
        // if it should migrate, we dont need to decrease from the current total bond amount
        pool_info.total_bond_amount = pool_info.total_bond_amount.checked_sub(amount)?;
    }

    // Update rewards info
    reward_info.bond_amount = reward_info.bond_amount.checked_sub(amount)?;

    if reward_info.bond_amount.is_zero() && should_migrate {
        store_is_migrated(storage, &asset_key, staker_addr)?;
    }

    if reward_info.pending_reward.is_zero() && reward_info.bond_amount.is_zero() {
        // if pending_withdraw is not empty, then return reward_assets to withdraw money
        reward_assets = reward_info
            .pending_withdraw
            .into_iter()
            .map(|ra| ra.to_normal(api))
            .collect::<StdResult<Vec<Asset>>>()?;

        rewards_store(storage, staker_addr).remove(&asset_key);
        // remove staker from the pool
        stakers_store(storage, &asset_key).remove(staker_addr);
    } else {
        rewards_store(storage, staker_addr).save(&asset_key, &reward_info)?;
    }

    // Update pool info
    store_pool_info(storage, &asset_key, &pool_info)?;

    Ok((staking_token, reward_assets))
}
