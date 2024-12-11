use cosmwasm_schema::{cw_serde, QueryResponses};

use cosmwasm_std::{
    coin, to_json_binary, Addr, Api, CosmosMsg, QuerierWrapper, StdResult, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use oraiswap_v3::PoolKey;

use crate::asset::AssetInfo;

#[cw_serde]
pub struct InstantiateMsg {
    pub factory_addr: Addr,
    pub factory_addr_v2: Addr,
    pub oraiswap_v3: Addr,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub enum SwapOperation {
    // swap cw20 token
    OraiSwap {
        offer_asset_info: AssetInfo,
        ask_asset_info: AssetInfo,
    },
    SwapV3 {
        pool_key: PoolKey,
        x_to_y: bool,
    },
}

impl SwapOperation {
    pub fn get_target_asset_info(&self, api: &dyn Api) -> AssetInfo {
        match self {
            SwapOperation::OraiSwap { ask_asset_info, .. } => ask_asset_info.clone(),
            SwapOperation::SwapV3 { pool_key, x_to_y } => match x_to_y {
                true => AssetInfo::from_denom(api, &pool_key.token_y),
                false => AssetInfo::from_denom(api, &pool_key.token_x),
            },
        }
    }
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    /// Execute multiple BuyOperation
    ExecuteSwapOperations {
        operations: Vec<SwapOperation>,
        minimum_receive: Option<Uint128>,
        to: Option<Addr>,
        affiliates: Option<Vec<Affiliate>>,
    },

    /// Internal use
    /// Swap all offer tokens to ask token
    ExecuteSwapOperation {
        operation: SwapOperation,
        to: Option<Addr>,
        sender: Addr,
    },
    /// Internal use
    /// Check the swap amount is exceed minimum_receive
    AssertMinimumReceiveAndTransfer {
        asset_info: AssetInfo,
        minimum_receive: Uint128,
        receiver: Addr,
        affiliates: Vec<Affiliate>,
    },
    UpdateConfig {
        factory_addr: Option<String>,
        factory_addr_v2: Option<String>,
        oraiswap_v3: Option<String>,
        owner: Option<String>,
    },
}

#[cw_serde]
pub enum Cw20HookMsg {
    ExecuteSwapOperations {
        operations: Vec<SwapOperation>,
        minimum_receive: Option<Uint128>,
        to: Option<String>,
        affiliates: Option<Vec<Affiliate>>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(SimulateSwapOperationsResponse)]
    SimulateSwapOperations {
        offer_amount: Uint128,
        operations: Vec<SwapOperation>,
    },
}

// We define a custom struct for each query response
#[cw_serde]
pub struct ConfigResponse {
    pub factory_addr: Addr,
    pub factory_addr_v2: Addr,
    pub oraiswap_v3: Addr,
}

// We define a custom struct for each query response
#[cw_serde]
pub struct SimulateSwapOperationsResponse {
    pub amount: Uint128,
}

#[cw_serde]
pub struct MixedRouterController(pub String);

impl MixedRouterController {
    pub fn addr(&self) -> String {
        self.0.clone()
    }

    /////////////////////////
    ///  Execute Messages ///
    /////////////////////////
    pub fn execute_operations(
        &self,
        swap_asset_info: AssetInfo,
        amount: Uint128,
        operations: Vec<SwapOperation>,
        minimum_receive: Option<Uint128>,
        swap_to: Option<Addr>,
        affiliates: Option<Vec<Affiliate>>,
    ) -> StdResult<CosmosMsg> {
        let cosmos_msg: CosmosMsg = match swap_asset_info {
            AssetInfo::Token { contract_addr } => WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Send {
                    contract: self.addr(),
                    amount,
                    msg: to_json_binary(&Cw20HookMsg::ExecuteSwapOperations {
                        operations,
                        minimum_receive,
                        to: swap_to.map(|to| to.into_string()),
                        affiliates,
                    })?,
                })?,
                funds: vec![],
            }
            .into(),
            AssetInfo::NativeToken { denom } => WasmMsg::Execute {
                contract_addr: self.addr(),
                msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
                    operations,
                    minimum_receive,
                    to: swap_to,
                    affiliates,
                })?,
                funds: vec![coin(amount.u128(), denom)],
            }
            .into(),
        };
        Ok(cosmos_msg)
    }

    /////////////////////////
    ///  Query Messages   ///
    /////////////////////////

    /// query if the given vamm is actually stored
    pub fn simulate_swap(
        &self,
        querier: &QuerierWrapper,
        offer_amount: Uint128,
        operations: Vec<SwapOperation>,
    ) -> StdResult<SimulateSwapOperationsResponse> {
        querier.query_wasm_smart(
            self.addr(),
            &QueryMsg::SimulateSwapOperations {
                offer_amount,
                operations,
            },
        )
    }
}

#[cw_serde]
pub struct Affiliate {
    pub basis_points_fee: Uint128,
    pub address: Addr,
}
