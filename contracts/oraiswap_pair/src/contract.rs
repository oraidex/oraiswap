use crate::state::{ADMIN, PAIR_INFO, WHITELISTED, WHITELISTED_TRADERS, WHITELISTED_WITHDRAW_LPS};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    from_json, to_json_binary, Addr, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Decimal256,
    Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg, Uint128,
    Uint256, WasmMsg,
};

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;
use integer_sqrt::IntegerSquareRoot;
use oraiswap::asset::{Asset, AssetInfo, AssetInfoRaw, PairInfoRaw};
use oraiswap::error::ContractError;
use oraiswap::oracle::OracleContract;
use oraiswap::pair::{
    compute_offer_amount, compute_swap, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg,
    PairResponse, PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse,
    DEFAULT_COMMISSION_RATE,
};
use oraiswap::querier::query_supply;
use oraiswap::response::MsgInstantiateContractResponse;
use std::convert::TryFrom;
use std::str::FromStr;

const INSTANTIATE_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let pair_info = &PairInfoRaw {
        // return infomation from oracle, update by multisig wallet
        oracle_addr: deps.api.addr_canonicalize(msg.oracle_addr.as_str())?,
        // the current contract address
        contract_addr: deps.api.addr_canonicalize(env.contract.address.as_str())?,
        // liquidity token address is ow20 to reward, mint and burn
        liquidity_token: CanonicalAddr::from(vec![]),
        // pair info
        asset_infos: [
            msg.asset_infos[0].to_raw(deps.api)?,
            msg.asset_infos[1].to_raw(deps.api)?,
        ],

        commission_rate: msg
            .commission_rate
            .unwrap_or(DEFAULT_COMMISSION_RATE.to_string()),
    };

    if let Some(admin) = msg.admin {
        ADMIN.save(deps.storage, &deps.api.addr_canonicalize(admin.as_str())?)?;
    }

    PAIR_INFO.save(deps.storage, pair_info)?;

    Ok(Response::new().add_submessage(SubMsg::reply_on_success(
        WasmMsg::Instantiate {
            admin: None,
            code_id: msg.token_code_id,
            msg: to_json_binary(&TokenInstantiateMsg {
                name: "oraiswap liquidity token".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                marketing: None,
            })?,
            funds: vec![],
            label: "lp".to_string(),
        },
        INSTANTIATE_REPLY_ID,
    )))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        // when transfer ow20 token to this contract
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        // add more liquidity
        ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance,
            receiver,
        } => provide_liquidity(deps, env, info, assets, slippage_tolerance, receiver),
        // swap token, can not swap native token directly
        ExecuteMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
        } => {
            if !offer_asset.is_native_token() {
                return Err(ContractError::Unauthorized {});
            }

            swap(
                deps,
                env,
                info.clone(),
                info.sender,
                offer_asset,
                belief_price,
                max_spread,
                to,
            )
        }
        ExecuteMsg::EnableWhitelist { status } => {
            // check permission
            assert_admin(deps.as_ref(), info.sender.to_string())?;
            WHITELISTED.save(deps.storage, &status)?;

            Ok(Response::default().add_attributes(vec![
                ("action", "enable_whitelisted"),
                ("status", &status.to_string()),
            ]))
        }
        ExecuteMsg::RegisterTrader { traders } => execute_register_traders(deps, info, traders),
        ExecuteMsg::DeregisterTrader { traders } => execute_deregister_traders(deps, info, traders),
        ExecuteMsg::RegisterWithdrawLp { providers } => {
            execute_register_withdraw_lps(deps, info, providers)
        }
        ExecuteMsg::DeregisterWithdrawLp { providers } => {
            execute_deregister_withdraw_lps(deps, info, providers)
        }
    }
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let contract_addr = info.sender.clone();

    match from_json(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Swap {
            belief_price,
            max_spread,
            to,
        }) => {
            // only asset contract can execute this message
            let mut authorized: bool = false;
            let config: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
            let pools: [Asset; 2] =
                config.query_pools(&deps.querier, deps.api, env.contract.address.clone())?;
            for pool in pools.iter() {
                if let AssetInfo::Token { contract_addr, .. } = &pool.info {
                    if info.sender.eq(contract_addr) {
                        authorized = true;
                        break;
                    }
                }
            }

            if !authorized {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = if let Some(to_addr) = to {
                Some(deps.api.addr_validate(to_addr.as_str())?)
            } else {
                None
            };

            swap(
                deps,
                env,
                info,
                Addr::unchecked(cw20_msg.sender),
                Asset {
                    info: AssetInfo::Token { contract_addr },
                    amount: cw20_msg.amount,
                },
                belief_price,
                max_spread,
                to_addr,
            )
        }
        // remove liquidity
        Ok(Cw20HookMsg::WithdrawLiquidity {}) => {
            let config: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
            if deps.api.addr_canonicalize(info.sender.as_str())? != config.liquidity_token {
                return Err(ContractError::Unauthorized {});
            }
            let sender_addr = deps.api.addr_validate(cw20_msg.sender.as_str())?;
            withdraw_liquidity(deps, env, info, sender_addr, cw20_msg.amount)
        }
        Err(err) => Err(ContractError::Std(err)),
    }
}

/// This just stores the result for future query
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    let data = msg.result.unwrap().data.unwrap();

    let res = MsgInstantiateContractResponse::try_from(data.as_slice()).map_err(|_| {
        StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
    })?;
    let liquidity_token = &res.address;

    let api = deps.api;
    PAIR_INFO.update(deps.storage, |mut meta| -> StdResult<_> {
        meta.liquidity_token = api.addr_canonicalize(liquidity_token)?;
        Ok(meta)
    })?;

    Ok(Response::new().add_attribute("liquidity_token_addr", liquidity_token))
}

/// CONTRACT - should approve contract to use the amount of token
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: [Asset; 2],
    slippage_tolerance: Option<Decimal>,
    receiver: Option<Addr>,
) -> Result<Response, ContractError> {
    // check pool is only open for whitelisted trader
    assert_is_open_for_whitelisted_trader(deps.as_ref(), info.sender.clone())?;

    for asset in assets.iter() {
        asset.assert_sent_native_token_balance(&info)?;
    }

    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    let mut pools: [Asset; 2] =
        pair_info.query_pools(&deps.querier, deps.api, env.contract.address.clone())?;

    let deposits: [Uint128; 2] = [
        assets
            .iter()
            .find(|a| a.info.eq(&pools[0].info))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
        assets
            .iter()
            .find(|a| a.info.eq(&pools[1].info))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
    ];

    let mut messages: Vec<CosmosMsg> = vec![];
    for (i, pool) in pools.iter_mut().enumerate() {
        // If the pool is token contract, then we need to execute TransferFrom msg to receive funds
        if let AssetInfo::Token { contract_addr, .. } = &pool.info {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_owned().into(),
                msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: deposits[i],
                })?,
                funds: vec![],
            }));
        } else {
            // If the asset is native token, balance is already increased
            // To calculated properly we should subtract user deposit from the pool
            pool.amount = pool.amount.checked_sub(deposits[i])?;
        }
    }

    // assert slippage tolerance
    assert_slippage_tolerance(&slippage_tolerance, &deposits, &pools)?;

    let liquidity_token = deps.api.addr_humanize(&pair_info.liquidity_token)?;
    let total_share = query_supply(&deps.querier, liquidity_token)?;
    let share = if total_share == Uint128::zero() {
        // Initial share = collateral amount
        Uint128::from((deposits[0].u128() * deposits[1].u128()).integer_sqrt())
    } else {
        // min(1, 2)
        // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_share / sqrt(pool_0 * pool_1))
        // == deposit_0 * total_share / pool_0
        // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_share / sqrt(pool_1 * pool_1))
        // == deposit_1 * total_share / pool_1
        std::cmp::min(
            deposits[0].multiply_ratio(total_share, pools[0].amount),
            deposits[1].multiply_ratio(total_share, pools[1].amount),
        )
    };

    // prevent providing free token
    if share.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // mint LP token to sender
    let receiver = receiver.unwrap_or(info.sender.clone());
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps
            .api
            .addr_humanize(&pair_info.liquidity_token)?
            .to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Mint {
            recipient: receiver.to_string(),
            amount: share,
        })?,
        funds: vec![],
    }));

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "provide_liquidity"),
        ("sender", info.sender.as_str()),
        ("receiver", receiver.as_str()),
        ("assets", &format!("{}, {}", assets[0], assets[1])),
        ("share", &share.to_string()),
    ]))
}

pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    // check pool is only open for whitelisted provider
    assert_is_open_for_whitelisted_withdraw_lp(deps.as_ref(), sender.clone())?;

    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    let liquidity_addr = deps.api.addr_humanize(&pair_info.liquidity_token)?;

    let pools: [Asset; 2] =
        pair_info.query_pools(&deps.querier, deps.api, env.contract.address.clone())?;
    let total_share: Uint128 = query_supply(&deps.querier, liquidity_addr)?;

    let share_ratio = Decimal::from_ratio(amount, total_share);
    if share_ratio.is_zero() {
        return Err(ContractError::InvalidZeroRatio {});
    }

    let refund_assets: Vec<Asset> = pools
        .iter()
        .map(|a| Asset {
            info: a.info.clone(),
            amount: a.amount * share_ratio,
        })
        .collect();

    let oracle_contract = OracleContract(deps.api.addr_humanize(&pair_info.oracle_addr)?);

    let messages = vec![
        refund_assets[0]
            .clone()
            .into_msg(Some(&oracle_contract), &deps.querier, sender.clone())?,
        refund_assets[1]
            .clone()
            .into_msg(Some(&oracle_contract), &deps.querier, sender.clone())?,
        // burn liquidity token
        WasmMsg::Execute {
            contract_addr: deps
                .api
                .addr_humanize(&pair_info.liquidity_token)?
                .to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Burn { amount })?,
            funds: vec![],
        }
        .into(),
    ];

    // update pool info
    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "withdraw_liquidity"),
        ("sender", sender.as_str()),
        ("withdrawn_share", &amount.to_string()),
        (
            "refund_assets",
            &format!("{}, {}", refund_assets[0], refund_assets[1]),
        ),
    ]))
}

/// CONTRACT - a user must do token approval
/// some params retrieving from oracle contract
#[allow(clippy::too_many_arguments)]
pub fn swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    // check pool is only open for whitelisted trader
    assert_is_open_for_whitelisted_trader(deps.as_ref(), sender.clone())?;

    offer_asset.assert_sent_native_token_balance(&info)?;

    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;

    let pools: [Asset; 2] =
        pair_info.query_pools(&deps.querier, deps.api, env.contract.address.clone())?;

    let offer_pool: Asset;
    let ask_pool: Asset;

    // If the asset balance is already increased
    // To calculated properly we should subtract user deposit from the pool
    if offer_asset.info.eq(&pools[0].info) {
        offer_pool = Asset {
            amount: pools[0].amount.checked_sub(offer_asset.amount)?,
            info: pools[0].info.clone(),
        };
        ask_pool = pools[1].clone();
    } else if offer_asset.info.eq(&pools[1].info) {
        offer_pool = Asset {
            amount: pools[1].amount.checked_sub(offer_asset.amount)?,
            info: pools[1].info.clone(),
        };
        ask_pool = pools[0].clone();
    } else {
        return Err(ContractError::AssetMismatch {});
    }

    let commission_rate = Decimal256::from_str(&pair_info.commission_rate)?;
    let offer_amount = offer_asset.amount;
    let (return_amount, spread_amount, commission_amount) = compute_swap(
        offer_pool.amount,
        ask_pool.amount,
        offer_amount,
        commission_rate,
    )?;

    // check max spread limit if exist
    assert_max_spread(
        belief_price,
        max_spread,
        offer_amount,
        return_amount + commission_amount,
        spread_amount,
    )?;

    // compute tax
    let return_asset = Asset {
        info: ask_pool.info.clone(),
        amount: return_amount,
    };

    let oracle_contract = OracleContract(deps.api.addr_humanize(&pair_info.oracle_addr)?);

    let tax_amount = return_asset.compute_tax(&oracle_contract, &deps.querier)?;
    let receiver = to.unwrap_or_else(|| sender.clone());

    // update oracle_contract
    let mut messages: Vec<CosmosMsg> = vec![];
    if !return_amount.is_zero() {
        messages.push(return_asset.into_msg(
            Some(&oracle_contract),
            &deps.querier,
            receiver.clone(),
        )?);
    }

    // 1. send collateral token from the contract to a user
    // 2. send inactive commission to collector
    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "swap"),
        ("sender", sender.as_str()),
        ("receiver", receiver.as_str()),
        ("offer_asset", &offer_asset.info.to_string()),
        ("ask_asset", &ask_pool.info.to_string()),
        ("offer_amount", &offer_amount.to_string()),
        ("return_amount", &return_amount.to_string()),
        ("tax_amount", &tax_amount.to_string()),
        ("spread_amount", &spread_amount.to_string()),
        ("commission_amount", &commission_amount.to_string()),
    ]))
}

fn execute_register_traders(
    deps: DepsMut,
    info: MessageInfo,
    traders: Vec<Addr>,
) -> Result<Response, ContractError> {
    // check permission
    assert_admin(deps.as_ref(), info.sender.to_string())?;

    // add traders to whitelist
    for trader in &traders {
        WHITELISTED_TRADERS.save(deps.storage, trader, &true)?;
    }

    Ok(Response::new().add_attributes(vec![("action", "register_trader")]))
}

fn execute_deregister_traders(
    deps: DepsMut,
    info: MessageInfo,
    traders: Vec<Addr>,
) -> Result<Response, ContractError> {
    // check permission
    assert_admin(deps.as_ref(), info.sender.to_string())?;

    // remove traders from whitelist
    for trader in &traders {
        WHITELISTED_TRADERS.save(deps.storage, trader, &false)?;
    }

    Ok(Response::new().add_attributes(vec![("action", "deregister_trader")]))
}

fn execute_register_withdraw_lps(
    deps: DepsMut,
    info: MessageInfo,
    providers: Vec<Addr>,
) -> Result<Response, ContractError> {
    // check permission
    assert_admin(deps.as_ref(), info.sender.to_string())?;

    // add traders to whitelist
    for provider in &providers {
        WHITELISTED_WITHDRAW_LPS.save(deps.storage, provider, &true)?;
    }

    Ok(Response::new().add_attributes(vec![("action", "register_whitelist_lp")]))
}

fn execute_deregister_withdraw_lps(
    deps: DepsMut,
    info: MessageInfo,
    providers: Vec<Addr>,
) -> Result<Response, ContractError> {
    // check permission
    assert_admin(deps.as_ref(), info.sender.to_string())?;

    // add traders to whitelist
    for provider in &providers {
        WHITELISTED_WITHDRAW_LPS.save(deps.storage, provider, &false)?;
    }

    Ok(Response::new().add_attributes(vec![("action", "deregister_whitelist_lp")]))
}

fn assert_admin(deps: Deps, sender: String) -> Result<(), ContractError> {
    let admin = ADMIN.may_load(deps.storage)?;

    if admin.is_none() {
        return Err(ContractError::Unauthorized {});
    }

    let sender_raw = deps.api.addr_canonicalize(&sender)?;

    if sender_raw != admin.unwrap() {
        return Err(ContractError::Unauthorized {});
    }

    Ok(())
}

fn assert_is_open_for_whitelisted_trader(deps: Deps, trader: Addr) -> Result<(), ContractError> {
    let is_whitelisted = WHITELISTED.may_load(deps.storage)?.unwrap_or(false);

    if !is_whitelisted {
        return Ok(());
    }

    let trader_whitelisted = WHITELISTED_TRADERS
        .may_load(deps.storage, &trader)?
        .unwrap_or(false);

    if !trader_whitelisted {
        return Err(ContractError::PoolWhitelisted {});
    }

    Ok(())
}

fn assert_is_open_for_whitelisted_withdraw_lp(
    deps: Deps,
    provider: Addr,
) -> Result<(), ContractError> {
    let is_whitelisted = WHITELISTED.may_load(deps.storage)?.unwrap_or(false);

    if !is_whitelisted {
        return Ok(());
    }

    let provider_whitelisted = WHITELISTED_WITHDRAW_LPS
        .may_load(deps.storage, &provider)?
        .unwrap_or(false);

    let trader_whitelisted = WHITELISTED_TRADERS
        .may_load(deps.storage, &provider)?
        .unwrap_or(false);

    if provider_whitelisted {
        return Ok(());
    }

    if trader_whitelisted {
        return Ok(());
    }

    Err(ContractError::PoolWhitelisted {})
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Pair {} => Ok(to_json_binary(&query_pair_info(deps)?)?),
        QueryMsg::Pool {} => Ok(to_json_binary(&query_pool(deps)?)?),
        QueryMsg::Simulation { offer_asset } => {
            Ok(to_json_binary(&query_simulation(deps, offer_asset)?)?)
        }
        QueryMsg::ReverseSimulation { ask_asset } => {
            Ok(to_json_binary(&query_reverse_simulation(deps, ask_asset)?)?)
        }
        QueryMsg::TraderIsWhitelisted { trader } => {
            Ok(to_json_binary(&query_trader_is_whitelisted(deps, trader)?)?)
        }
        QueryMsg::Admin {} => Ok(to_json_binary(&query_admin(deps)?)?),
    }
}

fn query_trader_is_whitelisted(deps: Deps, trader: Addr) -> StdResult<bool> {
    Ok(assert_is_open_for_whitelisted_trader(deps, trader).is_ok())
}

fn query_admin(deps: Deps) -> StdResult<String> {
    let admin = ADMIN.may_load(deps.storage)?;
    Ok(match admin {
        None => String::default(),
        Some(admin) => deps.api.addr_humanize(&admin)?.to_string(),
    })
}

pub fn query_pair_info(deps: Deps) -> StdResult<PairResponse> {
    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    pair_info
        .to_normal(deps.api)
        .map(|info| PairResponse { info })
}

pub fn query_pool(deps: Deps) -> Result<PoolResponse, ContractError> {
    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    let contract_addr = deps.api.addr_humanize(&pair_info.contract_addr)?;
    let assets: [Asset; 2] = pair_info.query_pools(&deps.querier, deps.api, contract_addr)?;
    let total_share: Uint128 = query_supply(
        &deps.querier,
        deps.api.addr_humanize(&pair_info.liquidity_token)?,
    )?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

pub fn query_simulation(
    deps: Deps,
    offer_asset: Asset,
) -> Result<SimulationResponse, ContractError> {
    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;

    let contract_addr = deps.api.addr_humanize(&pair_info.contract_addr)?;
    let pools: [Asset; 2] = pair_info.query_pools(&deps.querier, deps.api, contract_addr)?;

    let offer_pool: Asset;
    let ask_pool: Asset;
    if offer_asset.info.eq(&pools[0].info) {
        offer_pool = pools[0].clone();
        ask_pool = pools[1].clone();
    } else if offer_asset.info.eq(&pools[1].info) {
        offer_pool = pools[1].clone();
        ask_pool = pools[0].clone();
    } else {
        return Err(ContractError::AssetMismatch {});
    }

    let commission_rate = Decimal256::from_str(&pair_info.commission_rate)?;
    let (return_amount, spread_amount, commission_amount) = compute_swap(
        offer_pool.amount,
        ask_pool.amount,
        offer_asset.amount,
        commission_rate,
    )?;

    Ok(SimulationResponse {
        return_amount,
        spread_amount,
        commission_amount,
    })
}

pub fn query_reverse_simulation(
    deps: Deps,
    ask_asset: Asset,
) -> Result<ReverseSimulationResponse, ContractError> {
    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;

    let contract_addr = deps.api.addr_humanize(&pair_info.contract_addr)?;
    let pools: [Asset; 2] = pair_info.query_pools(&deps.querier, deps.api, contract_addr)?;

    let offer_pool: Asset;
    let ask_pool: Asset;
    if ask_asset.info.eq(&pools[0].info) {
        ask_pool = pools[0].clone();
        offer_pool = pools[1].clone();
    } else if ask_asset.info.eq(&pools[1].info) {
        ask_pool = pools[1].clone();
        offer_pool = pools[0].clone();
    } else {
        return Err(ContractError::AssetMismatch {});
    }

    let commission_rate = Decimal256::from_str(&pair_info.commission_rate)?;
    let (offer_amount, spread_amount, commission_amount) = compute_offer_amount(
        offer_pool.amount,
        ask_pool.amount,
        ask_asset.amount,
        commission_rate,
    )?;

    Ok(ReverseSimulationResponse {
        offer_amount,
        spread_amount,
        commission_amount,
    })
}

pub fn amount_of(coins: &[Coin], denom: String) -> Uint128 {
    match coins.iter().find(|x| x.denom == denom) {
        Some(coin) => coin.amount,
        None => Uint128::zero(),
    }
}

/// If `belief_price` and `max_spread` both are given,
/// we compute new spread else we just use oraiswap
/// spread to check `max_spread`
pub fn assert_max_spread(
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    offer_amount: Uint128,
    return_amount: Uint128,
    spread_amount: Uint128,
) -> Result<(), ContractError> {
    let offer_amount: Uint256 = offer_amount.into();
    let return_amount: Uint256 = return_amount.into();
    let spread_amount: Uint256 = spread_amount.into();

    if let (Some(max_spread), Some(belief_price)) = (max_spread, belief_price) {
        let belief_price: Decimal256 = belief_price.into();
        let max_spread: Decimal256 = max_spread.into();
        // mul with belief_price inv
        let expected_return = offer_amount * (Decimal256::one() / belief_price);

        let spread_amount = if expected_return > return_amount {
            expected_return - return_amount
        } else {
            Uint256::zero()
        };

        if return_amount < expected_return
            && Decimal256::from_ratio(spread_amount, expected_return) > max_spread
        {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    } else if let Some(max_spread) = max_spread {
        let max_spread: Decimal256 = max_spread.into();
        if Decimal256::from_ratio(spread_amount, return_amount + spread_amount) > max_spread {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    }

    Ok(())
}

fn assert_slippage_tolerance(
    slippage_tolerance: &Option<Decimal>,
    deposits: &[Uint128; 2],
    pools: &[Asset; 2],
) -> Result<(), ContractError> {
    if let Some(slippage_tolerance) = *slippage_tolerance {
        let slippage_tolerance: Decimal256 = slippage_tolerance.into();
        if slippage_tolerance > Decimal256::one() {
            return Err(ContractError::InvalidExceedOneSlippage {});
        }

        let one_minus_slippage_tolerance = Decimal256::one() - slippage_tolerance;
        let deposits: [Uint256; 2] = [deposits[0].into(), deposits[1].into()];
        let pools: [Uint256; 2] = [pools[0].amount.into(), pools[1].amount.into()];

        // Ensure each prices are not dropped as much as slippage tolerance rate
        if Decimal256::from_ratio(deposits[0], deposits[1]) * one_minus_slippage_tolerance
            > Decimal256::from_ratio(pools[0], pools[1])
            || Decimal256::from_ratio(deposits[1], deposits[0]) * one_minus_slippage_tolerance
                > Decimal256::from_ratio(pools[1], pools[0])
        {
            return Err(ContractError::MaxSlippageAssertion {});
        }
    }

    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    if let Some(admin) = msg.admin {
        let admin_canonical = deps.api.addr_canonicalize(&admin)?;
        ADMIN.save(deps.storage, &admin_canonical)?;
    }
    if let Some(asset_infos) = msg.asset_infos {
        let mut pair_info = PAIR_INFO.load(deps.storage)?;
        let asset_infos_raw: [AssetInfoRaw; 2] = [
            asset_infos[0].to_raw(deps.api).unwrap(),
            asset_infos[1]
                .to_raw(deps.api)
                .map_err(ContractError::Std)
                .unwrap(),
        ];
        pair_info.asset_infos = asset_infos_raw;
        PAIR_INFO.save(deps.storage, &pair_info)?;
    }
    Ok(Response::default())
}
