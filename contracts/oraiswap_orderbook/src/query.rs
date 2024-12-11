use crate::order::{matching_order, SLIPPAGE_DEFAULT};
use crate::orderbook::{Order, OrderBook};
use crate::state::{
    read_config, read_last_order_id, read_order, read_orderbook, read_orderbooks, read_orders,
    read_orders_with_indexer, PREFIX_ORDER_BY_BIDDER, PREFIX_ORDER_BY_DIRECTION,
    PREFIX_ORDER_BY_PRICE, PREFIX_TICK,
};
use cosmwasm_std::{Decimal, Deps, Order as OrderBy, StdError, StdResult, Storage, Uint128};
use oraiswap::error::ContractError;
use oraiswap::orderbook::{OrderStatus, SimulateMarketOrderResponse};
use std::convert::{TryFrom, TryInto};
use std::str::FromStr;

use oraiswap::asset::{pair_key, AssetInfo};
use oraiswap::{
    orderbook::{
        LastOrderIdResponse, OrderBookResponse, OrderBooksResponse, OrderDirection, OrderFilter,
        OrderResponse, OrdersResponse, TickResponse, TicksResponse,
    },
    querier::calc_range_start,
};

use cosmwasm_storage::ReadonlyBucket;

use crate::state::{DEFAULT_LIMIT, MAX_LIMIT};

type FilterFn = Box<dyn Fn(&OrderDirection) -> bool>;

pub fn query_order(
    deps: Deps,
    asset_infos: [AssetInfo; 2],
    order_id: u64,
) -> StdResult<OrderResponse> {
    let pair_key = pair_key(&[
        asset_infos[0].to_raw(deps.api)?,
        asset_infos[1].to_raw(deps.api)?,
    ]);
    let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;
    let order = read_order(deps.storage, &pair_key, order_id)?;

    order.to_response(
        deps.api,
        orderbook_pair.base_coin_info.to_normal(deps.api)?,
        orderbook_pair.quote_coin_info.to_normal(deps.api)?,
    )
}

pub fn query_orders(
    deps: Deps,
    asset_infos: [AssetInfo; 2],
    direction: Option<OrderDirection>,
    filter: OrderFilter,
    start_after: Option<u64>,
    limit: Option<u32>,
    order_by: Option<i32>,
) -> StdResult<OrdersResponse> {
    let order_by = order_by.and_then(|val| OrderBy::try_from(val).ok());
    let pair_key = pair_key(&[
        asset_infos[0].to_raw(deps.api)?,
        asset_infos[1].to_raw(deps.api)?,
    ]);
    let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;

    let (direction_filter, direction_key): (FilterFn, Vec<u8>) = match direction {
        // copy value to closure
        Some(d) => (Box::new(move |x| d.eq(x)), d.as_bytes().to_vec()),
        None => (Box::new(|_| true), OrderDirection::Buy.as_bytes().to_vec()),
    };

    let orders: Option<Vec<Order>> = match filter {
        OrderFilter::Bidder(bidder_addr) => {
            let bidder_addr_raw = deps.api.addr_canonicalize(&bidder_addr)?;
            read_orders_with_indexer::<OrderDirection>(
                deps.storage,
                &[
                    PREFIX_ORDER_BY_BIDDER,
                    &pair_key,
                    bidder_addr_raw.as_slice(),
                ],
                direction_filter,
                start_after,
                limit,
                order_by,
            )?
        }
        OrderFilter::Tick {} => read_orders_with_indexer::<u64>(
            deps.storage,
            &[PREFIX_TICK, &pair_key, &direction_key],
            Box::new(|_| true),
            start_after,
            limit,
            order_by,
        )?,
        OrderFilter::Price(price) => {
            let price_key = price.atomics().to_be_bytes();
            read_orders_with_indexer::<OrderDirection>(
                deps.storage,
                &[PREFIX_ORDER_BY_PRICE, &pair_key, &price_key],
                direction_filter,
                start_after,
                limit,
                order_by,
            )?
        }
        OrderFilter::None => match direction {
            Some(_) => read_orders_with_indexer::<OrderDirection>(
                deps.storage,
                &[PREFIX_ORDER_BY_DIRECTION, &pair_key, &direction_key],
                direction_filter,
                start_after,
                limit,
                order_by,
            )?,
            None => Some(read_orders(
                deps.storage,
                &pair_key,
                start_after,
                limit,
                order_by,
            )?),
        },
    };

    let resp = OrdersResponse {
        orders: orders
            .unwrap_or_default()
            .iter()
            .map(|order| {
                order.to_response(
                    deps.api,
                    orderbook_pair.base_coin_info.to_normal(deps.api)?,
                    orderbook_pair.quote_coin_info.to_normal(deps.api)?,
                )
            })
            .collect::<StdResult<Vec<OrderResponse>>>()?,
    };

    Ok(resp)
}

pub fn query_last_order_id(deps: Deps) -> StdResult<LastOrderIdResponse> {
    let last_order_id = read_last_order_id(deps.storage)?;
    let resp = LastOrderIdResponse { last_order_id };
    Ok(resp)
}

pub fn query_orderbooks(
    deps: Deps,
    start_after: Option<Vec<u8>>,
    limit: Option<u32>,
    order_by: Option<i32>,
) -> StdResult<OrderBooksResponse> {
    let order_by = order_by.and_then(|val| OrderBy::try_from(val).ok());
    let order_books = read_orderbooks(deps.storage, start_after, limit, order_by)?;
    order_books
        .into_iter()
        .map(|ob| ob.to_response(deps.api))
        .collect::<StdResult<Vec<OrderBookResponse>>>()
        .map(|order_books| OrderBooksResponse { order_books })
}

pub fn query_orderbook(deps: Deps, asset_infos: [AssetInfo; 2]) -> StdResult<OrderBookResponse> {
    let pair_key = pair_key(&[
        asset_infos[0].to_raw(deps.api)?,
        asset_infos[1].to_raw(deps.api)?,
    ]);
    let ob = read_orderbook(deps.storage, &pair_key)?;
    ob.to_response(deps.api)
}

pub fn query_ticks_prices(
    storage: &dyn Storage,
    pair_key: &[u8],
    direction: OrderDirection,
    start_after: Option<Decimal>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> Vec<Decimal> {
    query_ticks_prices_with_end(
        storage,
        pair_key,
        direction,
        start_after,
        None,
        limit,
        order_by,
    )
}

pub fn query_ticks_prices_with_end(
    storage: &dyn Storage,
    pair_key: &[u8],
    direction: OrderDirection,
    start_after: Option<Decimal>,
    end: Option<Decimal>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> Vec<Decimal> {
    query_ticks_with_end(
        storage,
        pair_key,
        direction,
        start_after,
        end,
        limit,
        order_by,
    )
    .unwrap_or(TicksResponse { ticks: vec![] })
    .ticks
    .into_iter()
    .map(|tick| tick.price)
    .collect::<Vec<Decimal>>()
}

pub fn query_ticks_with_end(
    storage: &dyn Storage,
    pair_key: &[u8],
    direction: OrderDirection,
    start_after: Option<Decimal>,
    end: Option<Decimal>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<TicksResponse> {
    let position_bucket: ReadonlyBucket<u64> =
        ReadonlyBucket::multilevel(storage, &[PREFIX_TICK, pair_key, direction.as_bytes()]);

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = start_after.map(|id| id.atomics().to_be_bytes().to_vec());
    let end = end.map(|id| id.atomics().to_be_bytes().to_vec());

    let (start, end, order_by) = match order_by {
        // start_after < x <= end
        Some(OrderBy::Ascending) => (
            calc_range_start(start_after),
            calc_range_start(end),
            OrderBy::Ascending,
        ),
        // start_after < x <= end
        _ => (end, start_after, OrderBy::Descending),
    };

    let ticks = position_bucket
        .range(start.as_deref(), end.as_deref(), order_by)
        .take(limit)
        .map(|item| {
            let (k, total_orders) = item?;
            let price = Decimal::raw(u128::from_be_bytes(k.try_into().unwrap()));
            Ok(TickResponse {
                price,
                total_orders,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(TicksResponse { ticks })
}

pub fn query_tick(
    storage: &dyn Storage,
    pair_key: &[u8],
    direction: OrderDirection,
    price: Decimal,
) -> StdResult<TickResponse> {
    let price_key = price.atomics().to_be_bytes();
    let total_orders =
        ReadonlyBucket::<u64>::multilevel(storage, &[PREFIX_TICK, pair_key, direction.as_bytes()])
            .load(&price_key)?;

    Ok(TickResponse {
        price,
        total_orders,
    })
}

pub fn get_price_info_for_market_order(
    storage: &dyn Storage,
    direction: OrderDirection,
    orderbook_pair: &OrderBook,
    offer_amount: Uint128,
    slippage: Decimal,
) -> Option<(Decimal, Decimal, Uint128)> {
    match direction {
        OrderDirection::Buy => orderbook_pair
            .lowest_price(storage, OrderDirection::Sell)
            .map(|(lowest_sell_price, _)| {
                (
                    lowest_sell_price,
                    lowest_sell_price * (Decimal::one() + slippage),
                    offer_amount * Decimal::one().atomics() / lowest_sell_price.atomics(),
                )
            }),
        OrderDirection::Sell => orderbook_pair
            .highest_price(storage, OrderDirection::Buy)
            .map(|(highest_buy_price, _)| {
                (
                    highest_buy_price,
                    highest_buy_price * (Decimal::one() - slippage),
                    offer_amount * highest_buy_price,
                )
            }),
    }
}

pub fn query_simulate_market_order(
    deps: Deps,
    direction: OrderDirection,
    asset_infos: [AssetInfo; 2],
    slippage: Option<Decimal>,
    offer_amount: Uint128,
) -> StdResult<SimulateMarketOrderResponse> {
    let config = read_config(deps.storage)?;

    let pair_key = pair_key(&[
        asset_infos[0].to_raw(deps.api)?,
        asset_infos[1].to_raw(deps.api)?,
    ]);
    let orderbook_pair = read_orderbook(deps.storage, &pair_key)?;

    let slippage = slippage.unwrap_or(
        orderbook_pair
            .spread
            .unwrap_or(Decimal::from_str(SLIPPAGE_DEFAULT)?),
    );

    let (_best_price, price_threshold, max_ask_amount) = match get_price_info_for_market_order(
        deps.storage,
        direction,
        &orderbook_pair,
        offer_amount,
        slippage,
    ) {
        Some(data) => data,
        None => {
            return Err(StdError::generic_err(
                ContractError::NoMatchedPrice {}.to_string(),
            ))
        }
    };

    // fake a order
    let user_orders = Order {
        order_id: 0,
        status: OrderStatus::Open,
        direction,
        bidder_addr: config.reward_address,
        offer_amount,
        ask_amount: max_ask_amount,
        filled_ask_amount: Uint128::zero(),
        filled_offer_amount: Uint128::zero(),
    };

    let (user_order_with_fee, _) =
        matching_order(deps, orderbook_pair, &user_orders, price_threshold)?;

    Ok(SimulateMarketOrderResponse {
        receive: user_order_with_fee.filled_ask_amount,
        refunds: offer_amount.checked_sub(user_order_with_fee.filled_offer_amount)?,
    })
}
