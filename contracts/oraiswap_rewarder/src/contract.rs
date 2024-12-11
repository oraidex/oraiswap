use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, QuerierWrapper, Response,
    StdError, StdResult, Uint128, WasmMsg,
};

use crate::state::{
    read_config, read_last_distributed, store_config, store_last_distributed, Config,
};

use oraiswap::staking::{
    ExecuteMsg as StakingExecuteMsg, QueryPoolInfoResponse, RewardsPerSecResponse,
};
use oraiswap::staking::{QueryMsg as StakingQueryMsg, RewardMsg};

use oraiswap::rewarder::{
    ConfigResponse, DistributionInfoResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    RewardAmountPerSecondResponse,
};

// 600 seconds default
const DEFAULT_DISTRIBUTION_INTERVAL: u64 = 600;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    store_config(
        deps.storage,
        &Config {
            init_time: env.block.time.seconds(),
            owner: deps.api.addr_canonicalize(info.sender.as_str())?,
            staking_contract: deps.api.addr_canonicalize(msg.staking_contract.as_str())?,
            distribution_interval: msg
                .distribution_interval
                .unwrap_or(DEFAULT_DISTRIBUTION_INTERVAL),
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::UpdateConfig {
            owner,
            staking_contract,
            distribution_interval,
        } => update_config(deps, info, owner, staking_contract, distribution_interval),

        ExecuteMsg::Distribute { staking_tokens } => distribute(deps, env, staking_tokens),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<Addr>,
    staking_contract: Option<Addr>,
    distribution_interval: Option<u64>,
) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(owner.as_str())?;
    }

    if let Some(staking_contract) = staking_contract {
        config.staking_contract = deps.api.addr_canonicalize(staking_contract.as_str())?;
    }

    if let Some(distribution_interval) = distribution_interval {
        config.distribution_interval = distribution_interval;
    }

    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

/// Distribute
/// Anyone can execute distribute operation to distribute
pub fn distribute(deps: DepsMut, env: Env, staking_tokens: Vec<Addr>) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let staking_contract = deps.api.addr_humanize(&config.staking_contract)?;
    let now = env.block.time.seconds();
    let mut rewards: Vec<RewardMsg> = vec![];
    for staking_token in staking_tokens {
        let asset_key = deps.api.addr_canonicalize(staking_token.as_str())?.to_vec();
        // default is init time
        let last_distributed = read_last_distributed(deps.storage, &asset_key)
            .unwrap_or(now - config.distribution_interval - 1);

        let last_time_elapsed = now - last_distributed;
        if last_time_elapsed < config.distribution_interval {
            // Cannot distribute reward tokens before interval, process next one
            continue;
        }

        // store last distributed
        store_last_distributed(deps.storage, &asset_key, now)?;

        // reward amount per second for a pool
        let reward_amount = _read_pool_reward_per_sec(
            &deps.querier,
            staking_contract.clone(),
            staking_token.clone(),
        );
        // no need to create a new distribute msg if the reward amount is 0
        if reward_amount.is_zero() {
            continue;
        }

        // get total reward amount for a pool
        let distribution_amount = Uint128::from(reward_amount.u128() * (last_time_elapsed as u128));

        // we will accumulate all rewards of a pool into a reward info pool. After that, we will re-calculate the percent of each reward token later in withdraw reward
        rewards.push(RewardMsg {
            staking_token,
            total_accumulation_amount: distribution_amount,
        });
    }

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_contract.to_string(),
            msg: to_json_binary(&StakingExecuteMsg::DepositReward { rewards })?,
            funds: vec![],
        }))
        .add_attribute("action", "distribute"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::DistributionInfo { staking_token } => {
            to_json_binary(&query_distribution_info(deps, staking_token)?)
        }
        QueryMsg::RewardAmountPerSec { staking_token } => {
            to_json_binary(&query_reward_amount_per_sec(deps, staking_token)?)
        }
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let state = read_config(deps.storage)?;
    let resp = ConfigResponse {
        owner: deps.api.addr_humanize(&state.owner)?,
        staking_contract: deps.api.addr_humanize(&state.staking_contract)?,
        distribution_interval: state.distribution_interval,
    };

    Ok(resp)
}

pub fn query_distribution_info(
    deps: Deps,
    staking_token: Addr,
) -> StdResult<DistributionInfoResponse> {
    let asset_key = deps.api.addr_canonicalize(staking_token.as_str())?.to_vec();
    let last_distributed = read_last_distributed(deps.storage, &asset_key)?;
    let resp = DistributionInfoResponse { last_distributed };

    Ok(resp)
}

pub fn query_reward_amount_per_sec(
    deps: Deps,
    staking_token: Addr,
) -> StdResult<RewardAmountPerSecondResponse> {
    let state = read_config(deps.storage)?;
    let reward_amount = _read_pool_reward_per_sec(
        &deps.querier,
        deps.api.addr_humanize(&state.staking_contract)?,
        staking_token,
    );

    Ok(RewardAmountPerSecondResponse { reward_amount })
}

fn _read_pool_reward_per_sec(
    querier: &QuerierWrapper,
    staking_contract: Addr,
    staking_token: Addr,
) -> Uint128 {
    let res: StdResult<RewardsPerSecResponse> = querier.query_wasm_smart(
        staking_contract,
        &StakingQueryMsg::RewardsPerSec { staking_token },
    );
    // default is zero
    res.map(|res| res.assets.iter().map(|a| a.amount).sum())
        .unwrap_or_default()
}

pub fn read_staking_tokens(
    querier: &QuerierWrapper,
    staking_contract: Addr,
) -> StdResult<Vec<String>> {
    let res: Vec<QueryPoolInfoResponse> =
        querier.query_wasm_smart(staking_contract, &StakingQueryMsg::GetPoolsInformation {})?;

    Ok(res.into_iter().map(|res| res.asset_key).collect())
}
