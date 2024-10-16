#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    from_json, to_json_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order as OrderBy, Response, StdError, StdResult, Uint128,
};
use cw_utils::one_coin;
use oraiswap::error::ContractError;

use crate::order::{
    cancel_order, get_paid_and_quote_assets, remove_pair, submit_market_order, submit_order,
};
use crate::orderbook::OrderBook;
use crate::query::{
    query_last_order_id, query_order, query_orderbook, query_orderbooks, query_orders,
    query_simulate_market_order, query_tick, query_ticks_with_end,
};
use crate::state::{
    init_last_order_id, read_config, read_orderbook, store_config, store_orderbook, validate_admin,
};
use cw_controllers::Hooks;

use cw20::Cw20ReceiveMsg;
use oraiswap::asset::{pair_key, Asset, AssetInfo};
use oraiswap::orderbook::{
    ContractInfo, ContractInfoResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg,
    OrderDirection, QueryMsg,
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:oraiswap_orderbook";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// default commission rate = 0.1 %
const DEFAULT_COMMISSION_RATE: &str = "0.001";

/// Hooks controller for the base asset holding whitelist
pub const WHITELIST_TRADER: Hooks = Hooks::new("whitelist_TRADER");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let creator = deps.api.addr_canonicalize(info.sender.as_str())?;
    let config = ContractInfo {
        name: msg.name.unwrap_or(CONTRACT_NAME.to_string()),
        version: msg.version.unwrap_or(CONTRACT_VERSION.to_string()),
        operator: match msg.operator {
            Some(addr) => Some(deps.api.addr_canonicalize(&addr)?),
            None => None,
        },
        // admin should be multisig
        admin: if let Some(admin) = msg.admin {
            deps.api.addr_canonicalize(admin.as_str())?
        } else {
            creator
        },
        commission_rate: msg
            .commission_rate
            .unwrap_or(DEFAULT_COMMISSION_RATE.to_string()),
        reward_address: deps.api.addr_canonicalize(msg.reward_address.as_str())?,
        is_paused: false,
    };

    store_config(deps.storage, &config)?;

    init_last_order_id(deps.storage)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // check has paused
    check_paused(deps.as_ref(), &msg)?;

    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Pause {} => {
            let mut config = read_config(deps.storage)?;
            validate_admin(deps.api, &config.admin, info.sender.as_str())?;
            config.is_paused = true;
            store_config(deps.storage, &config)?;

            Ok(Response::new().add_attribute("action", "pause"))
        }
        ExecuteMsg::Unpause {} => {
            let mut config = read_config(deps.storage)?;
            validate_admin(deps.api, &config.admin, info.sender.as_str())?;
            config.is_paused = false;
            store_config(deps.storage, &config)?;

            Ok(Response::new().add_attribute("action", "unpause"))
        }
        ExecuteMsg::UpdateAdmin { admin } => execute_update_admin(deps, info, admin),
        ExecuteMsg::UpdateOperator { operator } => execute_update_operator(deps, info, operator),
        ExecuteMsg::UpdateConfig {
            reward_address,
            commission_rate,
        } => execute_update_config(deps, info, reward_address, commission_rate),
        ExecuteMsg::CreateOrderBookPair {
            base_coin_info,
            quote_coin_info,
            spread,
            min_quote_coin_amount,
            refund_threshold,
            min_offer_to_fulfilled,
            min_ask_to_fulfilled,
        } => execute_create_pair(
            deps,
            info,
            base_coin_info,
            quote_coin_info,
            spread,
            min_quote_coin_amount,
            refund_threshold,
            min_offer_to_fulfilled,
            min_ask_to_fulfilled,
        ),
        ExecuteMsg::UpdateOrderBookPair {
            asset_infos,
            spread,
            min_quote_coin_amount,
            refund_threshold,
            min_offer_to_fulfilled,
            min_ask_to_fulfilled,
        } => {
            validate_admin(
                deps.api,
                &read_config(deps.storage)?.admin,
                info.sender.as_str(),
            )?;
            let pair_key = pair_key(&[
                asset_infos[0].to_raw(deps.api)?,
                asset_infos[1].to_raw(deps.api)?,
            ]);
            let mut orderbook_pair = read_orderbook(deps.storage, &pair_key)?;
            if let Some(spread) = spread {
                if spread >= Decimal::one() {
                    return Err(ContractError::SlippageMustLessThanOne { slippage: spread });
                }
            }
            orderbook_pair.spread = spread;

            // update new minium quote amount threshold
            if let Some(min_quote_coin_amount) = min_quote_coin_amount {
                orderbook_pair.min_quote_coin_amount = min_quote_coin_amount;
            }

            // update new refunds threshold
            if let Some(refund_threshold) = refund_threshold {
                orderbook_pair.refund_threshold = Some(refund_threshold);
            }

            if let Some(min_offer_to_fulfilled) = min_offer_to_fulfilled {
                orderbook_pair.min_offer_to_fulfilled = Some(min_offer_to_fulfilled);
            }

            if let Some(min_ask_to_fulfilled) = min_ask_to_fulfilled {
                orderbook_pair.min_ask_to_fulfilled = Some(min_ask_to_fulfilled);
            }

            store_orderbook(deps.storage, &pair_key, &orderbook_pair)?;
            Ok(Response::new().add_attributes(vec![("action", "update_orderbook_data")]))
        }
        ExecuteMsg::SubmitOrder { direction, assets } => {
            let pair_key = pair_key(&[
                assets[0].to_raw(deps.api)?.info,
                assets[1].to_raw(deps.api)?.info,
            ]);
            let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;

            // if sell then paid asset must be ask asset, this way we've just assumed that we offer usdt and ask for orai
            // for execute order, it is direct match(user has known it is buy or sell) so no order is needed
            // Buy: wanting ask asset(orai) => paid offer asset(usdt)
            // Sell: paid ask asset(orai) => wating offer asset(usdt)
            let (paid_assets, quote_asset) =
                get_paid_and_quote_assets(deps.api, &orderbook_pair, assets, direction)?;

            paid_assets[0].assert_if_asset_is_native_token()?;
            paid_assets[0].assert_sent_native_token_balance(&info)?;

            // require minimum amount for quote asset
            if quote_asset.amount.lt(&orderbook_pair.min_quote_coin_amount) {
                return Err(ContractError::TooSmallQuoteAsset {
                    quote_coin: quote_asset.info.to_string(),
                    min_quote_amount: orderbook_pair.min_quote_coin_amount,
                });
            }

            // then submit order
            submit_order(deps, &orderbook_pair, info.sender, direction, paid_assets)
        }
        ExecuteMsg::SubmitMarketOrder {
            direction,
            asset_infos,
            slippage,
        } => {
            let pair_key = pair_key(&[
                asset_infos[0].to_raw(deps.api)?,
                asset_infos[1].to_raw(deps.api)?,
            ]);
            let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;

            let offer_asset_info = match direction {
                OrderDirection::Buy => orderbook_pair.quote_coin_info.to_normal(deps.api)?,
                OrderDirection::Sell => orderbook_pair.base_coin_info.to_normal(deps.api)?,
            };

            // get asset_info
            let funds = one_coin(&info)?;

            let provided_asset = Asset {
                info: AssetInfo::NativeToken { denom: funds.denom },
                amount: funds.amount,
            };

            if offer_asset_info != provided_asset.info {
                return Err(ContractError::InvalidFunds {});
            }

            // submit market order
            submit_market_order(
                deps,
                &orderbook_pair,
                info.sender,
                direction,
                provided_asset,
                slippage,
            )
        }
        ExecuteMsg::CancelOrder {
            order_id,
            asset_infos,
        } => cancel_order(deps, info, order_id, asset_infos),
        ExecuteMsg::RemoveOrderBookPair { asset_infos } => remove_pair(deps, info, asset_infos),
        ExecuteMsg::WithdrawToken { asset } => {
            let contract_info = read_config(deps.storage)?;
            validate_admin(deps.api, &contract_info.admin, info.sender.as_str())?;
            let msg = asset.into_msg(
                None,
                &deps.querier,
                deps.api.addr_humanize(&contract_info.admin)?,
            )?;
            Ok(Response::new().add_message(msg).add_attributes(vec![
                ("action", "withdraw_token"),
                ("token", &asset.to_string()),
            ]))
        }
        ExecuteMsg::WhitelistTrader { trader } => execute_whitelist_trader(deps, info, trader),
        ExecuteMsg::RemoveTrader { trader } => execute_remove_trader(deps, info, trader),
    }
}

fn check_paused(deps: Deps, msg: &ExecuteMsg) -> Result<(), ContractError> {
    if let Ok(config) = read_config(deps.storage) {
        if config.is_paused {
            match msg {
                ExecuteMsg::UpdateAdmin { admin: _ }
                | ExecuteMsg::UpdateConfig { .. }
                | ExecuteMsg::Pause {}
                | ExecuteMsg::Unpause {}
                | ExecuteMsg::UpdateOrderBookPair { .. } => {
                    // still not paused
                }
                _ => return Err(ContractError::Paused {}),
            }
        }
    }
    Ok(())
}

pub fn execute_whitelist_trader(
    deps: DepsMut,
    info: MessageInfo,
    trader: Addr,
) -> Result<Response, ContractError> {
    let contract_info = read_config(deps.storage)?;
    validate_admin(deps.api, &contract_info.admin, info.sender.as_str())?;

    WHITELIST_TRADER
        .add_hook(deps.storage, trader.clone())
        .map_err(|error| StdError::generic_err(error.to_string()))?;

    Ok(Response::new().add_attributes(vec![
        ("action", "whitelist_trader"),
        ("trader", trader.as_str()),
    ]))
}

pub fn execute_remove_trader(
    deps: DepsMut,
    info: MessageInfo,
    trader: Addr,
) -> Result<Response, ContractError> {
    let contract_info = read_config(deps.storage)?;
    validate_admin(deps.api, &contract_info.admin, info.sender.as_str())?;

    WHITELIST_TRADER
        .remove_hook(deps.storage, trader.clone())
        .map_err(|error| StdError::generic_err(error.to_string()))?;

    Ok(Response::new().add_attributes(vec![
        ("action", "remove_trader"),
        ("trader", trader.as_str()),
    ]))
}

pub fn execute_update_admin(
    deps: DepsMut,
    info: MessageInfo,
    admin: Addr,
) -> Result<Response, ContractError> {
    let mut contract_info = read_config(deps.storage)?;
    validate_admin(deps.api, &contract_info.admin, info.sender.as_str())?;

    // update new admin
    contract_info.admin = deps.api.addr_canonicalize(admin.as_str())?;
    store_config(deps.storage, &contract_info)?;

    Ok(Response::new().add_attributes(vec![("action", "execute_update_admin")]))
}

pub fn execute_update_operator(
    deps: DepsMut,
    info: MessageInfo,
    operator: Option<String>,
) -> Result<Response, ContractError> {
    let mut contract_info = read_config(deps.storage)?;
    validate_admin(deps.api, &contract_info.admin, info.sender.as_str())?;

    // if None then no operator to receive reward
    contract_info.operator = match operator {
        Some(addr) => Some(deps.api.addr_canonicalize(&addr)?),
        None => None,
    };

    store_config(deps.storage, &contract_info)?;

    Ok(Response::new().add_attributes(vec![("action", "execute_update_operator")]))
}

pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    reward_address: Option<Addr>,
    commission_rate: Option<String>,
) -> Result<Response, ContractError> {
    let mut contract_info = read_config(deps.storage)?;
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;

    // check authorized
    if contract_info.admin.ne(&sender_addr) {
        return Err(ContractError::Unauthorized {});
    }

    // update new reward address
    if let Some(reward_address) = reward_address {
        contract_info.reward_address = deps.api.addr_canonicalize(reward_address.as_str())?;
    }

    // update new commission rate
    if let Some(commission_rate) = commission_rate {
        contract_info.commission_rate = commission_rate;
    }

    store_config(deps.storage, &contract_info)?;
    Ok(Response::new().add_attributes(vec![("action", "execute_update_config")]))
}

pub fn execute_create_pair(
    deps: DepsMut,
    info: MessageInfo,
    base_coin_info: AssetInfo,
    quote_coin_info: AssetInfo,
    spread: Option<Decimal>,
    min_quote_coin_amount: Uint128,
    refund_threshold: Option<Uint128>,
    min_offer_to_fulfilled: Option<Uint128>,
    min_ask_to_fulfilled: Option<Uint128>,
) -> Result<Response, ContractError> {
    let contract_info = read_config(deps.storage)?;
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;

    // check authorized
    if contract_info.admin.ne(&sender_addr) {
        return Err(ContractError::Unauthorized {});
    }

    let pair_key = pair_key(&[
        base_coin_info.to_raw(deps.api)?,
        quote_coin_info.to_raw(deps.api)?,
    ]);

    let ob = read_orderbook(deps.storage, &pair_key);

    // Orderbook already exists
    if ob.is_ok() {
        return Err(ContractError::OrderBookAlreadyExists {});
    }

    if let Some(spread) = spread {
        if spread >= Decimal::one() {
            return Err(ContractError::SlippageMustLessThanOne { slippage: spread });
        }
    }

    let order_book = OrderBook {
        base_coin_info: base_coin_info.to_raw(deps.api)?,
        quote_coin_info: quote_coin_info.to_raw(deps.api)?,
        spread,
        min_quote_coin_amount,
        refund_threshold,
        min_offer_to_fulfilled,
        min_ask_to_fulfilled,
    };
    store_orderbook(deps.storage, &pair_key, &order_book)?;

    Ok(Response::new().add_attributes(vec![
        ("action", "create_orderbook_pair"),
        ("pair", &format!("{} - {}", base_coin_info, quote_coin_info)),
        ("spread", &format!("{:.5}", spread.unwrap_or_default())),
        ("min_quote_coin_amount", &min_quote_coin_amount.to_string()),
    ]))
}

pub fn receive_cw20(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(cw20_msg.sender.as_str())?;

    let provided_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: info.sender,
        },
        amount: cw20_msg.amount,
    };

    match from_json(&cw20_msg.msg) {
        Ok(Cw20HookMsg::SubmitOrder { direction, assets }) => {
            let pair_key = pair_key(&[
                assets[0].to_raw(deps.api)?.info,
                assets[1].to_raw(deps.api)?.info,
            ]);
            let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;

            // validate offer asset is valid
            let is_valid_funds = match direction {
                OrderDirection::Buy => orderbook_pair
                    .quote_coin_info
                    .eq(&provided_asset.info.to_raw(deps.api)?),
                OrderDirection::Sell => orderbook_pair
                    .base_coin_info
                    .eq(&provided_asset.info.to_raw(deps.api)?),
            };

            if !is_valid_funds {
                return Err(ContractError::InvalidFunds {});
            }

            let (paid_assets, quote_asset) =
                get_paid_and_quote_assets(deps.api, &orderbook_pair, assets, direction)?;

            if paid_assets[0] != provided_asset {
                return Err(ContractError::InvalidFunds {});
            };

            // require minimum amount for quote asset
            if quote_asset.amount.lt(&orderbook_pair.min_quote_coin_amount) {
                return Err(ContractError::TooSmallQuoteAsset {
                    quote_coin: quote_asset.info.to_string(),
                    min_quote_amount: orderbook_pair.min_quote_coin_amount,
                });
            }

            // then submit order
            submit_order(deps, &orderbook_pair, sender, direction, paid_assets)
        }
        Ok(Cw20HookMsg::SubmitMarketOrder {
            direction,
            asset_infos,
            slippage,
        }) => {
            let pair_key = pair_key(&[
                asset_infos[0].to_raw(deps.api)?,
                asset_infos[1].to_raw(deps.api)?,
            ]);

            let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;

            let offer_asset_info = match direction {
                OrderDirection::Buy => orderbook_pair.quote_coin_info.to_normal(deps.api)?,
                OrderDirection::Sell => orderbook_pair.base_coin_info.to_normal(deps.api)?,
            };

            if offer_asset_info != provided_asset.info {
                return Err(ContractError::InvalidFunds {});
            }

            let sender_addr = deps.api.addr_validate(&cw20_msg.sender)?;

            // submit market order
            submit_market_order(
                deps,
                &orderbook_pair,
                sender_addr,
                direction,
                provided_asset,
                slippage,
            )
        }
        Err(_) => Err(ContractError::InvalidCw20HookMessage {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::ContractInfo {} => to_json_binary(&query_contract_info(deps)?),
        QueryMsg::Order {
            order_id,
            asset_infos,
        } => to_json_binary(&query_order(deps, asset_infos, order_id)?),
        QueryMsg::OrderBook { asset_infos } => to_json_binary(&query_orderbook(deps, asset_infos)?),
        QueryMsg::OrderBooks {
            start_after,
            limit,
            order_by,
        } => to_json_binary(&query_orderbooks(deps, start_after, limit, order_by)?),
        QueryMsg::Orders {
            asset_infos,
            direction,
            filter,
            start_after,
            limit,
            order_by,
        } => to_json_binary(&query_orders(
            deps,
            asset_infos,
            direction,
            filter,
            start_after,
            limit,
            order_by,
        )?),
        QueryMsg::LastOrderId {} => to_json_binary(&query_last_order_id(deps)?),
        QueryMsg::Tick {
            price,
            asset_infos,
            direction,
        } => to_json_binary(&query_tick(
            deps.storage,
            &pair_key(&[
                asset_infos[0].to_raw(deps.api)?,
                asset_infos[1].to_raw(deps.api)?,
            ]),
            direction,
            price,
        )?),
        QueryMsg::Ticks {
            asset_infos,
            direction,
            start_after,
            end,
            limit,
            order_by,
        } => to_json_binary(&query_ticks_with_end(
            deps.storage,
            &pair_key(&[
                asset_infos[0].to_raw(deps.api)?,
                asset_infos[1].to_raw(deps.api)?,
            ]),
            direction,
            start_after,
            end,
            limit,
            order_by.and_then(|val| OrderBy::try_from(val).ok()),
        )?),
        QueryMsg::MidPrice { asset_infos } => {
            let pair_key = pair_key(&[
                asset_infos[0].to_raw(deps.api)?,
                asset_infos[1].to_raw(deps.api)?,
            ]);
            let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;
            let mid_price = match (
                orderbook_pair.highest_price(deps.storage, OrderDirection::Buy),
                orderbook_pair.lowest_price(deps.storage, OrderDirection::Sell),
            ) {
                (Some((best_buy_price, _)), Some((best_sell_price, _))) => {
                    (best_buy_price + best_sell_price) * Decimal::from_ratio(1u128, 2u128)
                }
                _ => {
                    return Err(StdError::generic_err(
                        ContractError::NoMatchedPrice {}.to_string(),
                    ))
                }
            };

            to_json_binary(&mid_price)
        }
        QueryMsg::SimulateMarketOrder {
            direction,
            asset_infos,
            slippage,
            offer_amount,
        } => to_json_binary(&query_simulate_market_order(
            deps,
            direction,
            asset_infos,
            slippage,
            offer_amount,
        )?),
        QueryMsg::WhitelistedTraders {} => {
            to_json_binary(&WHITELIST_TRADER.query_hooks(deps)?.hooks)
        }
    }
}

pub fn query_contract_info(deps: Deps) -> StdResult<ContractInfoResponse> {
    let info = read_config(deps.storage)?;
    Ok(ContractInfoResponse {
        version: info.version,
        name: info.name,
        admin: deps.api.addr_humanize(&info.admin)?,
        commission_rate: info.commission_rate,
        reward_address: deps.api.addr_humanize(&info.reward_address)?,
        operator: if let Some(operator) = info.operator {
            Some(deps.api.addr_humanize(&operator)?)
        } else {
            None
        },
        is_paused: info.is_paused,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
