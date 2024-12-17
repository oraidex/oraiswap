use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Decimal};

use crate::asset::{Asset, AssetInfo, PairInfo};

#[cw_serde]
pub struct InstantiateMsg {
    /// Pair contract code ID, which is used to
    pub pair_code_id: u64,
    pub token_code_id: u64,
    pub oracle_addr: Addr,
    pub commission_rate: Option<String>,
    pub operator_fee: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// UpdateConfig update relevant code IDs
    UpdateConfig {
        owner: Option<String>,
        token_code_id: Option<u64>,
        pair_code_id: Option<u64>,
    },
    /// CreatePair instantiates pair contract
    CreatePair {
        /// Asset infos
        asset_infos: [AssetInfo; 2],
        pair_admin: Option<String>,
        operator: Option<String>,
        provide_liquidity: Option<ProvideLiquidityParams>,
    },
    AddPair {
        pair_info: PairInfo,
    },
    MigrateContract {
        contract_addr: String,
        new_code_id: u64,
        msg: Binary,
    },
    ProvideLiquidity {
        assets: [Asset; 2],
        receiver: Addr,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(PairInfo)]
    Pair { asset_infos: [AssetInfo; 2] },
    #[returns(PairsResponse)]
    Pairs {
        start_after: Option<[AssetInfo; 2]>,
        limit: Option<u32>,
    },
}

// We define a custom struct for each query response
#[cw_serde]
pub struct ConfigResponse {
    pub owner: Addr,
    pub oracle_addr: Addr,
    pub pair_code_id: u64,
    pub token_code_id: u64,
}

/// We currently take no arguments for migrations
#[cw_serde]
pub struct MigrateMsg {
    pub owner: Addr,
    pub pair_code_id: u64,
    pub token_code_id: u64,
    pub oracle_addr: Addr,
    pub commission_rate: Option<String>,
    pub operator_fee: Option<String>,
}

// We define a custom struct for each query response
#[cw_serde]
pub struct PairsResponse {
    pub pairs: Vec<PairInfo>,
}

#[cw_serde]
pub struct ProvideLiquidityParams {
    pub assets: [Asset; 2],
    pub receiver: Option<Addr>,
}
