use std::str::FromStr;

use crate::contract::WHITELIST_TRADER;
use crate::orderbook::{Executor, Order, OrderBook, OrderWithFee};
use crate::query::get_price_info_for_market_order;
use crate::state::{
    increase_last_order_id, read_config, read_order, read_orderbook, read_reward, remove_order,
    remove_orderbook, store_order, store_reward, PREFIX_TICK,
};
use cosmwasm_std::{
    attr, Addr, Api, Attribute, CanonicalAddr, CosmosMsg, Decimal, Deps, DepsMut, Event,
    MessageInfo, Order as OrderBy, Response, StdError, StdResult, Storage, Uint128,
};

use cosmwasm_storage::ReadonlyBucket;
use oraiswap::asset::{pair_key, Asset, AssetInfo};
use oraiswap::error::ContractError;
use oraiswap::orderbook::{OrderDirection, OrderStatus, Payment};

pub const MIN_VOLUME: u128 = 10u128;
const MIN_FEE: u128 = 1_000_000u128;
pub const SLIPPAGE_DEFAULT: &str = "0.01"; // spread default 1%
pub const REFUNDS_THRESHOLD: u128 = 100000u128; // 0.1

pub fn submit_order(
    deps: DepsMut,
    orderbook_pair: &OrderBook,
    sender: Addr,
    direction: OrderDirection,
    assets: [Asset; 2],
) -> Result<Response, ContractError> {
    assets[0].assert_if_asset_is_zero()?;
    assets[1].assert_if_asset_is_zero()?;

    let offer_amount = assets[0].amount;
    let ask_amount = assets[1].amount;

    if ask_amount.is_zero() {
        return Err(ContractError::AssetMustNotBeZero {});
    }

    let order_id = increase_last_order_id(deps.storage)?;

    store_order(
        deps.storage,
        &orderbook_pair.get_pair_key(),
        &Order {
            order_id,
            direction,
            bidder_addr: deps.api.addr_canonicalize(sender.as_str())?,
            offer_amount,
            ask_amount,
            filled_offer_amount: Uint128::zero(),
            filled_ask_amount: Uint128::zero(),
            status: OrderStatus::Open,
        },
        true,
    )?;

    // check if this order can be matched, then execute matching process
    let (matched, price) = match direction {
        OrderDirection::Buy => {
            let price = Decimal::from_ratio(offer_amount, ask_amount);

            match orderbook_pair.lowest_price(deps.storage, OrderDirection::Sell) {
                Some((lowest_sell_price, _)) => (lowest_sell_price <= price, price),
                None => (false, Decimal::zero()),
            }
        }
        OrderDirection::Sell => {
            let price = Decimal::from_ratio(ask_amount, offer_amount);

            match orderbook_pair.highest_price(deps.storage, OrderDirection::Buy) {
                Some((highest_buy_price, _)) => (highest_buy_price >= price, price),
                None => (false, Decimal::zero()),
            }
        }
    };

    let pair = format!(
        "{} - {}",
        &orderbook_pair.base_coin_info.to_normal(deps.api)?,
        &orderbook_pair.quote_coin_info.to_normal(deps.api)?
    );

    let response = if matched {
        process_matching(deps, sender.clone(), orderbook_pair, order_id, price)?
    } else {
        Response::new()
    };

    Ok(response.add_attributes(vec![
        ("action", "submit_order"),
        ("order_type", "limit"),
        ("pair", &pair),
        ("order_id", &order_id.to_string()),
        ("status", &format!("{:?}", OrderStatus::Open)),
        ("direction", &format!("{:?}", direction)),
        ("bidder_addr", sender.as_str()),
        (
            "offer_asset",
            &format!("{} {}", &assets[0].amount, &assets[0].info),
        ),
        (
            "ask_asset",
            &format!("{} {}", &assets[1].amount, &assets[1].info),
        ),
    ]))
}

pub fn submit_market_order(
    mut deps: DepsMut,
    orderbook_pair: &OrderBook,
    sender: Addr,
    direction: OrderDirection,
    offer_asset: Asset,
    slippage: Option<Decimal>,
) -> Result<Response, ContractError> {
    let order_id = increase_last_order_id(deps.storage)?;

    let slippage = slippage.unwrap_or(
        orderbook_pair
            .spread
            .unwrap_or(Decimal::from_str(SLIPPAGE_DEFAULT)?),
    );

    if slippage >= Decimal::one() {
        return Err(ContractError::SlippageMustLessThanOne { slippage });
    }

    // with market order, ask_amount will be maximum amount can receive
    let (_best_price, price_threshold, max_ask_amount) = match get_price_info_for_market_order(
        deps.storage,
        direction,
        orderbook_pair,
        offer_asset.amount,
        slippage,
    ) {
        Some(data) => data,
        None => return Err(ContractError::CannotCreateMarketOrder {}),
    };

    store_order(
        deps.storage,
        &orderbook_pair.get_pair_key(),
        &Order {
            order_id,
            direction,
            bidder_addr: deps.api.addr_canonicalize(sender.as_str())?,
            offer_amount: offer_asset.amount,
            ask_amount: max_ask_amount,
            filled_offer_amount: Uint128::zero(),
            filled_ask_amount: Uint128::zero(),
            status: OrderStatus::Open,
        },
        true,
    )?;

    // matching process
    let mut response = process_matching(
        deps.branch(),
        sender.clone(),
        orderbook_pair,
        order_id,
        price_threshold,
    )?;

    // after matching process, if order still exist => not fulfilled => refund
    let order = read_order(deps.storage, &orderbook_pair.get_pair_key(), order_id);

    if let Ok(order) = order {
        let refund_amount = order.offer_amount.checked_sub(order.filled_offer_amount)?;
        let refund_asset = Asset {
            info: offer_asset.info.clone(),
            amount: refund_amount,
        };
        // remove this order
        remove_order(deps.storage, &orderbook_pair.get_pair_key(), &order)?;
        response = response
            .add_attribute("refund_amount", refund_amount.to_string())
            .add_message(refund_asset.into_msg(None, &deps.querier, sender.clone())?)
    }

    Ok(response.add_attributes(vec![
        ("action", "submit_market_order"),
        ("order_type", "market"),
        (
            "pair",
            &format!(
                "{} - {}",
                &orderbook_pair.base_coin_info.to_normal(deps.api)?,
                &orderbook_pair.quote_coin_info.to_normal(deps.api)?
            ),
        ),
        ("order_id", &order_id.to_string()),
        ("status", &format!("{:?}", OrderStatus::Open)),
        ("direction", &format!("{:?}", direction)),
        ("bidder_addr", sender.as_str()),
        (
            "offer_asset",
            &format!("{} {}", &offer_asset.amount, &offer_asset.info),
        ),
    ]))
}

pub fn cancel_order(
    deps: DepsMut,
    info: MessageInfo,
    order_id: u64,
    asset_infos: [AssetInfo; 2],
) -> Result<Response, ContractError> {
    let pair_key = pair_key(&[
        asset_infos[0].to_raw(deps.api)?,
        asset_infos[1].to_raw(deps.api)?,
    ]);
    let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;
    let order = read_order(deps.storage, &pair_key, order_id)?;

    if order.bidder_addr != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(ContractError::Unauthorized {});
    }

    // Compute refund asset
    let left_offer_amount = order.offer_amount.checked_sub(order.filled_offer_amount)?;

    let bidder_refund = Asset {
        info: match order.direction {
            OrderDirection::Buy => orderbook_pair.quote_coin_info.to_normal(deps.api)?,
            OrderDirection::Sell => orderbook_pair.base_coin_info.to_normal(deps.api)?,
        },
        amount: left_offer_amount,
    };

    // Build refund msg
    let messages = if left_offer_amount > Uint128::zero() {
        vec![bidder_refund.clone().into_msg(
            None,
            &deps.querier,
            deps.api.addr_humanize(&order.bidder_addr)?,
        )?]
    } else {
        vec![]
    };

    remove_order(deps.storage, &pair_key, &order)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "cancel_order"),
        (
            "pair",
            &format!(
                "{} - {}",
                &orderbook_pair.base_coin_info.to_normal(deps.api)?,
                &orderbook_pair.quote_coin_info.to_normal(deps.api)?
            ),
        ),
        ("order_id", &order_id.to_string()),
        ("direction", &format!("{:?}", order.direction)),
        ("status", "Cancel"),
        (
            "bidder_addr",
            deps.api.addr_humanize(&order.bidder_addr)?.as_str(),
        ),
        ("offer_amount", &order.offer_amount.to_string()),
        ("ask_amount", &order.ask_amount.to_string()),
        ("bidder_refund", &bidder_refund.to_string()),
    ]))
}

fn to_events(order: &OrderWithFee, human_bidder: String) -> Event {
    let attrs: Vec<Attribute> = [
        attr("status", format!("{:?}", order.status)),
        attr("bidder_addr", human_bidder),
        attr("order_id", order.order_id.to_string()),
        attr("direction", format!("{:?}", order.direction)),
        attr("offer_amount", order.offer_amount.to_string()),
        attr("filled_offer_amount", order.filled_offer_amount.to_string()),
        attr("ask_amount", order.ask_amount.to_string()),
        attr("filled_ask_amount", order.filled_ask_amount.to_string()),
        attr("reward_fee", order.reward_fee),
        attr("filled_offer_this_round", order.filled_offer_this_round),
        attr("filled_ask_this_round", order.filled_ask_this_round),
    ]
    .to_vec();
    Event::new("matched_order").add_attributes(attrs)
}

fn process_reward(
    storage: &dyn Storage,
    pair_key: &[u8],
    address: CanonicalAddr,
    reward_assets: [Asset; 2],
) -> Executor {
    read_reward(storage, pair_key, &address)
        .unwrap_or_else(|_| Executor::new(address, reward_assets))
}

fn transfer_reward(
    deps: &DepsMut,
    executor: &mut Executor,
    total_reward: &mut Vec<String>,
    messages: &mut Vec<CosmosMsg>,
) -> StdResult<()> {
    for reward_asset in executor.reward_assets.iter_mut() {
        if reward_asset.amount.u128() >= MIN_FEE {
            messages.push(reward_asset.into_msg(
                None,
                &deps.querier,
                deps.api.addr_humanize(&executor.address)?,
            )?);
            total_reward.push(reward_asset.to_string());
            reward_asset.amount = Uint128::zero();
        }
    }
    Ok(())
}

fn process_list_trader(
    deps: &DepsMut,
    traders: Vec<Payment>,
    messages: &mut Vec<CosmosMsg>,
) -> StdResult<()> {
    let mut minimalist_trader: Vec<Payment> = vec![];
    for trader in traders {
        if let Some(existing_payment) = minimalist_trader
            .iter_mut()
            .find(|p| p.address == trader.address && p.asset.info.eq(&trader.asset.info))
        {
            existing_payment.asset.amount += trader.asset.amount;
        } else {
            minimalist_trader.push(trader);
        }
    }

    for trader in minimalist_trader {
        if !trader.asset.amount.is_zero() {
            messages.push(trader.asset.into_msg(
                None,
                &deps.querier,
                deps.api.addr_validate(trader.address.as_str())?,
            )?);
        }
    }

    Ok(())
}

pub fn calculate_fee(
    commission_rate: Decimal,
    direction: OrderDirection,
    trader_ask_asset: &mut Asset,
    reward: &mut Executor,
) -> StdResult<Uint128> {
    let reward_fee = trader_ask_asset.amount * commission_rate;

    match direction {
        OrderDirection::Buy => {
            reward.reward_assets[0].amount += reward_fee;
        }
        OrderDirection::Sell => {
            reward.reward_assets[1].amount += reward_fee;
        }
    }

    trader_ask_asset.amount = trader_ask_asset
        .amount
        .checked_sub(reward_fee)
        .unwrap_or_default();

    Ok(reward_fee)
}

fn process_payment_list_orders(
    deps: &Deps,
    commission_rate: Decimal,
    orderbook_pair: &OrderBook,
    orders: &mut Vec<OrderWithFee>,
    traders: &mut Vec<Payment>,
    reward: &mut Executor,
) -> StdResult<()> {
    for order in orders {
        let filled_offer = order.filled_offer_this_round;
        let filled_ask = order.filled_ask_this_round;

        if !filled_ask.is_zero() && !filled_offer.is_zero() {
            let mut trader_ask_asset = Asset {
                info: match order.direction {
                    OrderDirection::Buy => orderbook_pair.base_coin_info.to_normal(deps.api)?,
                    OrderDirection::Sell => orderbook_pair.quote_coin_info.to_normal(deps.api)?,
                },
                amount: filled_ask,
            };

            let bidder_addr = deps.api.addr_humanize(&order.bidder_addr)?;

            if !WHITELIST_TRADER.query_hook(*deps, bidder_addr.to_string())? {
                let reward_fee: Uint128 = calculate_fee(
                    commission_rate,
                    order.direction,
                    &mut trader_ask_asset,
                    reward,
                )?;
                order.reward_fee = reward_fee;
            }

            if !trader_ask_asset.amount.is_zero() {
                let trader_payment: Payment = Payment {
                    address: bidder_addr,
                    asset: Asset {
                        info: trader_ask_asset.info.clone(),
                        amount: trader_ask_asset.amount,
                    },
                };
                traders.push(trader_payment);
            }
        }
    }

    Ok(())
}

pub fn matching_order(
    deps: Deps,
    orderbook_pair: OrderBook,
    order: &Order,
    order_price: Decimal,
) -> StdResult<(OrderWithFee, Vec<OrderWithFee>)> {
    let pair_key = &orderbook_pair.get_pair_key();
    let matched_orders_direction = match order.direction {
        OrderDirection::Buy => OrderDirection::Sell,
        OrderDirection::Sell => OrderDirection::Buy,
    };

    let positions_bucket: ReadonlyBucket<u64> = ReadonlyBucket::multilevel(
        deps.storage,
        &[PREFIX_TICK, pair_key, matched_orders_direction.as_bytes()],
    );

    let mut user_order = OrderWithFee::from_order(order.to_owned());
    let mut orders_matched: Vec<OrderWithFee> = vec![];
    let sort_order = match order.direction {
        OrderDirection::Buy => OrderBy::Ascending,
        OrderDirection::Sell => OrderBy::Descending,
    };

    // check minimum offer & ask to mark a order as fulfilled
    let min_offer = orderbook_pair
        .min_offer_to_fulfilled
        .unwrap_or(Uint128::from(MIN_VOLUME));
    let min_ask = orderbook_pair
        .min_ask_to_fulfilled
        .unwrap_or(Uint128::from(MIN_VOLUME));

    let (user_min_offer_to_fulfilled, user_min_ask_to_fulfilled) = match order.direction {
        OrderDirection::Buy => (min_offer, min_ask),
        OrderDirection::Sell => (min_ask, min_offer),
    };

    // in matching process of buy order, we don't check minimum remaining amount to mark user order as fulfilled, but only with a small threshold
    let mut cursor = positions_bucket.range(None, None, sort_order);
    while let Some(Ok((k, _))) = cursor.next() {
        let match_price =
            Decimal::raw(u128::from_be_bytes(k.try_into().map_err(|_| {
                StdError::generic_err("Error converting bytes to u128")
            })?));

        match order.direction {
            OrderDirection::Buy => {
                if order_price < match_price {
                    break;
                }
            }
            OrderDirection::Sell => {
                if order_price > match_price {
                    break;
                }
            }
        }

        if user_order.is_fulfilled() {
            break;
        }

        if let Some(orders) = orderbook_pair.query_orders_by_price_and_direction(
            deps.storage,
            match_price,
            matched_orders_direction,
            None,
        ) {
            if orders.is_empty() {
                continue;
            }

            let match_orders_with_fees = OrderWithFee::from_orders(orders);

            for mut match_order in match_orders_with_fees {
                if user_order.is_fulfilled() {
                    break;
                }
                // remaining ask & offer of buy order
                let lef_user_ask = user_order
                    .ask_amount
                    .checked_sub(user_order.filled_ask_amount)?;
                let lef_user_offer = user_order
                    .offer_amount
                    .checked_sub(user_order.filled_offer_amount)?;

                // remaining offer & ask of sell order
                let lef_match_offer = match_order
                    .offer_amount
                    .checked_sub(match_order.filled_offer_amount)?;
                let lef_match_ask = match_order
                    .ask_amount
                    .checked_sub(match_order.filled_ask_amount)?;

                // ask_amount of user_order <= min(lef_buy_ask, lef_sell_offer)
                let mut user_ask_amount = Uint128::min(lef_user_ask, lef_match_offer);
                let mut user_offer_amount = match order.direction {
                    OrderDirection::Buy => user_ask_amount * match_price,
                    OrderDirection::Sell => {
                        user_ask_amount * Decimal::one().atomics() / match_price.atomics()
                    }
                };

                let min_user_offer_amount = Uint128::min(lef_user_offer, lef_match_ask);
                // if user_offer_amount > min_user_offer_amount, we need re calc ask_amount
                if user_offer_amount > min_user_offer_amount {
                    user_offer_amount = min_user_offer_amount;
                    user_ask_amount = match order.direction {
                        OrderDirection::Buy => {
                            user_offer_amount * Decimal::one().atomics() / match_price.atomics()
                        }
                        OrderDirection::Sell => user_offer_amount * match_price,
                    }
                }

                // with match order, since order direction is opposite to the user's order, so the params will be reverse
                match_order.fill_order(
                    user_offer_amount,
                    user_ask_amount,
                    user_min_offer_to_fulfilled,
                    user_min_ask_to_fulfilled,
                )?;
                user_order.fill_order(
                    user_ask_amount,
                    user_offer_amount,
                    user_min_ask_to_fulfilled,
                    user_min_offer_to_fulfilled,
                )?;

                orders_matched.push(match_order);
            }
        }
    }

    // recheck user order is fulfilled
    user_order.fill_order(
        Uint128::zero(),
        Uint128::zero(),
        user_min_ask_to_fulfilled,
        user_min_offer_to_fulfilled,
    )?;
    user_order.filled_offer_this_round = user_order.filled_offer_amount;
    user_order.filled_ask_this_round = user_order.filled_ask_amount;

    Ok((user_order, orders_matched))
}

pub fn process_matching(
    deps: DepsMut,
    _sender: Addr,
    orderbook_pair: &OrderBook,
    order_id: u64,
    price_threshold: Decimal,
) -> Result<Response, ContractError> {
    let contract_info = read_config(deps.storage)?;
    let commission_rate = Decimal::from_str(&contract_info.commission_rate)?;

    // get default operator to receive reward
    let pair_key = orderbook_pair.get_pair_key();

    let reward_assets = [
        Asset {
            info: orderbook_pair.base_coin_info.to_normal(deps.api)?,
            amount: Uint128::zero(),
        },
        Asset {
            info: orderbook_pair.quote_coin_info.to_normal(deps.api)?,
            amount: Uint128::zero(),
        },
    ];
    let mut reward = process_reward(
        deps.storage,
        &pair_key,
        contract_info.reward_address,
        reward_assets.clone(),
    );

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut list_trader: Vec<Payment> = vec![];
    let mut matched_events: Vec<Event> = vec![];
    let mut total_reward: Vec<String> = Vec::new();

    let order = read_order(deps.storage, &pair_key, order_id)?;

    let (offer_order_with_fee, mut matched_orders) = matching_order(
        deps.as_ref(),
        orderbook_pair.clone(),
        &order,
        price_threshold,
    )?;

    if matched_orders.is_empty() {
        return Ok(Response::default());
    }

    // process calc payment and fee
    matched_orders.push(offer_order_with_fee);
    process_payment_list_orders(
        &deps.as_ref(),
        commission_rate,
        orderbook_pair,
        &mut matched_orders,
        &mut list_trader,
        &mut reward,
    )?;

    let refund_threshold = orderbook_pair
        .refund_threshold
        .unwrap_or(Uint128::from(REFUNDS_THRESHOLD));

    for order_matched in matched_orders.iter_mut() {
        if order_matched.status != OrderStatus::Open {
            let refund_amount =
                order_matched.match_order(deps.storage, &pair_key, refund_threshold)?;

            if !refund_amount.is_zero() {
                let offer_asset_info = match order_matched.direction {
                    OrderDirection::Buy => orderbook_pair.quote_coin_info.to_normal(deps.api)?,
                    OrderDirection::Sell => orderbook_pair.base_coin_info.to_normal(deps.api)?,
                };

                let trader_refunds: Payment = Payment {
                    address: deps.api.addr_humanize(&order_matched.bidder_addr)?,
                    asset: Asset {
                        info: offer_asset_info,
                        amount: refund_amount,
                    },
                };

                list_trader.push(trader_refunds);
            }
            matched_events.push(to_events(
                order_matched,
                deps.api
                    .addr_humanize(&order_matched.bidder_addr)?
                    .to_string(),
            ));
        }
    }

    process_list_trader(&deps, list_trader, &mut messages)?;

    transfer_reward(&deps, &mut reward, &mut total_reward, &mut messages)?;

    store_reward(deps.storage, &pair_key, &reward)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![
            ("total_matched_orders", &matched_events.len().to_string()),
            ("executor_reward", &format!("{:?}", &total_reward)),
        ])
        .add_events(matched_events))
}

pub fn remove_pair(
    deps: DepsMut,
    info: MessageInfo,
    asset_infos: [AssetInfo; 2],
) -> Result<Response, ContractError> {
    let contract_info = read_config(deps.storage)?;
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;

    if contract_info.admin.ne(&sender_addr) {
        return Err(ContractError::Unauthorized {});
    }

    let pair_key = pair_key(&[
        asset_infos[0].to_raw(deps.api)?,
        asset_infos[1].to_raw(deps.api)?,
    ]);

    remove_orderbook(deps.storage, &pair_key);

    Ok(Response::new().add_attributes(vec![
        ("action", "remove_orderbook_pair"),
        (
            "pair",
            &format!("{} - {}", &asset_infos[0], &asset_infos[1]),
        ),
    ]))
}

pub fn get_paid_and_quote_assets(
    api: &dyn Api,
    orderbook_pair: &OrderBook,
    assets: [Asset; 2],
    direction: OrderDirection,
) -> StdResult<([Asset; 2], Asset)> {
    let mut assets_reverse = assets.clone();
    assets_reverse.reverse();
    let paid_assets: [Asset; 2];
    let quote_asset: Asset;

    if orderbook_pair.base_coin_info.to_normal(api)? == assets[0].info {
        paid_assets = match direction {
            OrderDirection::Buy => assets_reverse,
            OrderDirection::Sell => assets.clone(),
        };
        quote_asset = assets[1].clone();
    } else {
        paid_assets = match direction {
            OrderDirection::Buy => assets.clone(),
            OrderDirection::Sell => assets_reverse,
        };
        quote_asset = assets[0].clone();
    }
    Ok((paid_assets, quote_asset))
}
