use cosmwasm_schema::{cw_serde, QueryResponses};

use crate::asset::{Asset, AssetInfo};
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct InstantiateMsg {
    // default is sender
    pub owner: Option<Addr>,
    pub rewarder: Addr,
    pub minter: Option<Addr>,
    pub oracle_addr: Addr,
    pub factory_addr: Addr,
    pub base_denom: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),

    ////////////////////////
    /// Owner operations ///
    ////////////////////////
    UpdateConfig {
        rewarder: Option<Addr>,
        owner: Option<Addr>,
        migrate_store_status: Option<bool>,
    },
    RegisterAsset {
        staking_token: Addr,
    },
    DeprecateStakingToken {
        staking_token: Addr,
        new_staking_token: Addr,
    },
    // update rewards per second for an asset
    UpdateRewardsPerSec {
        staking_token: Addr,
        assets: Vec<Asset>,
    },
    // reward tokens are in amount proportionaly, and used by minter contract to update amounts after checking the balance, which
    // will be used as rewards for the specified asset's staking pool.
    DepositReward {
        rewards: Vec<RewardMsg>,
    },
    /// Owner can withdraw funds (native or CW20) held by the contract
    WithdrawFunds {
        assets: Vec<Asset>,
        receiver: Option<Addr>,
    },

    ////////////////////////
    /// User operations ///
    ////////////////////////
    Unbond {
        staking_token: Addr,
        amount: Uint128,
    },
    /// Withdraw pending rewards
    Withdraw {
        // If the asset token is not given, then all rewards are withdrawn
        staking_token: Option<Addr>,
    },
    // Withdraw for others in this pool, such as when rewards per second are changed for the pool
    WithdrawOthers {
        staking_token: Option<Addr>,
        staker_addrs: Vec<Addr>,
    },

    /// Provides liquidity and automatically stakes the LP tokens
    AutoStake {
        assets: [Asset; 2],
        slippage_tolerance: Option<Decimal>,
    },
    /// Hook to stake the minted LP tokens
    AutoStakeHook {
        staking_token: Addr,
        staker_addr: Addr,
        prev_staking_token_amount: Uint128,
    },
}

#[cw_serde]
pub enum Cw20HookMsg {
    // this call from LP token contract
    Bond {},
}

/// We currently take no arguments for migrations
#[cw_serde]
pub struct MigrateMsg {}

/// We currently take no arguments for migrations
#[cw_serde]
pub struct AmountInfo {
    pub asset_info: AssetInfo,
    pub amount: Uint128,
    // pub new_staking_token: Addr,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(PoolInfoResponse)]
    PoolInfo { staking_token: Addr },
    #[returns(RewardsPerSecResponse)]
    RewardsPerSec { staking_token: Addr },
    #[returns(RewardInfoResponse)]
    RewardInfo {
        staker_addr: Addr,
        staking_token: Option<Addr>,
    },
    #[returns(Vec<RewardInfoResponse>)]
    // Query all staker belong to the pool
    RewardInfos {
        staking_token: Addr,
        start_after: Option<Addr>,
        limit: Option<u32>,
        // so can convert or throw error
        order: Option<i32>,
    },
    #[returns(Vec<QueryPoolInfoResponse>)]
    GetPoolsInformation {},
    #[returns(cosmwasm_std::Binary)]
    QueryOldStore { store_type: OldStoreType },
}

// We define a custom struct for each query response
#[cw_serde]
pub struct ConfigResponse {
    pub owner: Addr,
    pub rewarder: Addr,
    pub oracle_addr: Addr,
    pub factory_addr: Addr,
    pub base_denom: String,
}

#[cw_serde]
pub struct RewardsPerSecResponse {
    pub assets: Vec<Asset>,
}

// We define a custom struct for each query response
#[cw_serde]
pub struct PoolInfoResponse {
    pub staking_token: Addr,
    pub total_bond_amount: Uint128,
    pub reward_index: Decimal,
    pub pending_reward: Uint128,
    pub migration_index_snapshot: Option<Decimal>,
    pub migration_deprecated_staking_token: Option<Addr>,
}

// We define a custom struct for each query response
#[cw_serde]
pub struct RewardInfoResponse {
    pub staker_addr: Addr,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[cw_serde]
pub struct RewardInfoResponseItem {
    pub staking_token: Addr,
    pub bond_amount: Uint128,
    pub pending_reward: Uint128,
    pub pending_withdraw: Vec<Asset>,
    // returns true if the position should be closed to keep receiving rewards
    // with the new lp token
    pub should_migrate: Option<bool>,
}

#[cw_serde]
pub struct RewardMsg {
    pub staking_token: Addr,
    pub total_accumulation_amount: Uint128,
}

#[cw_serde]
pub struct QueryPoolInfoResponse {
    pub asset_key: String,
    pub pool_info: PoolInfoResponse,
}

#[cw_serde]
pub enum OldStoreType {
    Pools {},
    Stakers { asset_info: AssetInfo },
    Rewards { staker: String },
    IsMigrated { staker: String },
    RewardsPerSec {},
}
