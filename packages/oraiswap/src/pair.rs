use std::convert::TryInto;

use crate::{
    asset::{Asset, AssetInfo, PairInfo},
    error::ContractError,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal256, StdError, Uint256};

use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

/// Default commission rate == 0.3%
/// in the future need to update ?
pub const DEFAULT_COMMISSION_RATE: &str = "0.003";

/// Default operator fee == 0.1%
pub const DEFAULT_OPERATOR_FEE: &str = "0.001";

#[cw_serde]
pub struct InstantiateMsg {
    /// Asset infos
    pub asset_infos: [AssetInfo; 2],
    /// Token contract code id for initialization
    pub token_code_id: u64,

    /// Oracle contract for query oracle information
    pub oracle_addr: Addr,

    pub commission_rate: Option<String>,
    // admin
    pub admin: Option<Addr>,
    // operator fee
    pub operator_fee: Option<String>,
    // operator
    pub operator: Option<Addr>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    /// ProvideLiquidity a user provides pool liquidity
    ProvideLiquidity {
        assets: [Asset; 2],
        slippage_tolerance: Option<Decimal>,
        receiver: Option<Addr>,
    },
    /// Swap an offer asset to the other
    Swap {
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<Addr>,
    },
    /// Turn on/off only whitelisted address can interact with pool
    EnableWhitelist {
        status: bool,
    },
    // Add trader to  whitelist
    RegisterTrader {
        traders: Vec<Addr>,
    },

    // remove trader from whitelist
    DeregisterTrader {
        traders: Vec<Addr>,
    },
    RegisterWithdrawLp {
        providers: Vec<Addr>,
    },
    DeregisterWithdrawLp {
        providers: Vec<Addr>,
    },
    UpdatePoolInfo {
        commission_rate: Option<String>,
        operator_fee: Option<String>,
    },
    // update operator
    UpdateOperator {
        operator: Option<String>,
    },
}

#[cw_serde]
pub enum PairExecuteMsgCw20 {
    /// Swap an offer asset to the other
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<Addr>,
    },
}

#[cw_serde]
pub enum Cw20HookMsg {
    /// Sell a given amount of asset
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    WithdrawLiquidity {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(PairResponse)]
    Pair {},
    #[returns(PoolResponse)]
    Pool {},
    #[returns(SimulationResponse)]
    Simulation { offer_asset: Asset },
    #[returns(ReverseSimulationResponse)]
    ReverseSimulation { ask_asset: Asset },
    #[returns(bool)]
    TraderIsWhitelisted { trader: Addr },
    #[returns(String)]
    Admin {},
    #[returns(String)]
    Operator {},
}

// We define a custom struct for each query response
#[cw_serde]
pub struct PoolResponse {
    pub assets: [Asset; 2],
    pub total_share: Uint128,
}

#[cw_serde]
pub struct PairResponse {
    pub info: PairInfo,
}

/// SimulationResponse returns swap simulation response
#[cw_serde]
pub struct SimulationResponse {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
    #[serde(default)]
    pub operator_fee_amount: Uint128,
}

/// ReverseSimulationResponse returns reverse swap simulation response
#[cw_serde]
pub struct ReverseSimulationResponse {
    pub offer_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
}

/// We currently take no arguments for migrations
#[cw_serde]
pub struct MigrateMsg {
    pub admin: Option<String>,
    pub asset_infos: Option<[AssetInfo; 2]>,
}

pub fn compute_swap(
    offer_pool: Uint128,
    ask_pool: Uint128,
    offer_amount: Uint128,
    commission_rate: Decimal256,
    operator_fee: Decimal256,
) -> Result<(Uint128, Uint128, Uint128, Uint128), ContractError> {
    if offer_pool.is_zero() {
        return Err(ContractError::OfferPoolIsZero {});
    }

    // convert to uint256
    let offer_pool: Uint256 = offer_pool.into();
    let ask_pool: Uint256 = ask_pool.into();
    let offer_amount: Uint256 = offer_amount.into();

    // offer => ask
    // let cp = offer_pool * ask_pool;
    // ask_amount = (ask_pool - cp / (offer_pool + offer_amount)) * (1 - commission_rate)

    let return_amount = (ask_pool * offer_amount) / (offer_pool + offer_amount);

    // calculate spread & commission
    let spread_amount = offer_amount.multiply_ratio(ask_pool, offer_pool) - return_amount;

    let commission_amount = return_amount * commission_rate;
    let operator_fee = return_amount * operator_fee;

    // commission will be absorbed to pool
    let return_amount = return_amount - commission_amount - operator_fee;
    Ok((
        return_amount
            .try_into()
            .map_err(|err| StdError::from(err))?,
        spread_amount
            .try_into()
            .map_err(|err| StdError::from(err))?,
        commission_amount
            .try_into()
            .map_err(|err| StdError::from(err))?,
        operator_fee.try_into().map_err(|err| StdError::from(err))?,
    ))
}

pub fn compute_offer_amount(
    offer_pool: Uint128,
    ask_pool: Uint128,
    ask_amount: Uint128,
    commission_rate: Decimal256,
) -> Result<(Uint128, Uint128, Uint128), ContractError> {
    let offer_pool: Uint256 = offer_pool.into();
    let ask_pool: Uint256 = ask_pool.into();
    let ask_amount: Uint256 = ask_amount.into();

    // ask => offer
    // offer_amount = cp / (ask_pool - ask_amount / (1 - commission_rate)) - offer_pool
    let cp = offer_pool.checked_mul(ask_pool)?;

    let before_commission_deduction = ask_amount
        * (Decimal256::one()
            .checked_div(Decimal256::one().checked_sub(commission_rate)?)
            .map_err(|err| StdError::generic_err(err.to_string()))?);

    let offer_amount: Uint256 = Uint256::one()
        .multiply_ratio(cp, ask_pool.checked_sub(before_commission_deduction)?)
        .checked_sub(offer_pool)?;

    let before_spread_deduction: Uint256 =
        offer_amount * Decimal256::from_ratio(ask_pool, offer_pool);

    // default return zero, and we can catch later instead of panic
    let spread_amount = before_spread_deduction
        .checked_sub(before_commission_deduction)
        .unwrap_or_default();

    let commission_amount = before_commission_deduction * commission_rate;

    Ok((
        offer_amount.try_into().map_err(StdError::from)?,
        spread_amount.try_into().map_err(StdError::from)?,
        commission_amount.try_into().map_err(StdError::from)?,
    ))
}
