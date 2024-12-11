use std::convert::TryInto;

use cosmwasm_schema::cw_serde;
use cosmwasm_storage::ReadonlyBucket;
use oraiswap::{
    asset::{pair_key_from_asset_keys, Asset, AssetInfo, AssetInfoRaw},
    orderbook::{OrderBookResponse, OrderDirection, OrderResponse, OrderStatus},
};

use cosmwasm_std::{Api, CanonicalAddr, Decimal, Order as OrderBy, StdResult, Storage, Uint128};

use crate::{
    order::{MIN_VOLUME, REFUNDS_THRESHOLD},
    query::{query_ticks_prices, query_ticks_prices_with_end},
    state::{
        read_orders, read_orders_with_indexer, remove_order, store_order, PREFIX_ORDER_BY_PRICE,
        PREFIX_TICK,
    },
};

#[cw_serde]
pub struct Order {
    pub order_id: u64,
    pub status: OrderStatus,
    pub direction: OrderDirection, // if direction is sell then offer => sell asset, ask => buy asset
    pub bidder_addr: CanonicalAddr,
    pub offer_amount: Uint128,
    pub ask_amount: Uint128,
    pub filled_offer_amount: Uint128,
    pub filled_ask_amount: Uint128,
}

#[cw_serde]
pub struct OrderWithFee {
    pub order_id: u64,
    pub status: OrderStatus,
    pub direction: OrderDirection, // if direction is sell then offer => sell asset, ask => buy asset
    pub bidder_addr: CanonicalAddr,
    pub offer_amount: Uint128,
    pub ask_amount: Uint128,
    pub filled_offer_amount: Uint128,
    pub filled_ask_amount: Uint128,
    pub reward_fee: Uint128,
    pub filled_offer_this_round: Uint128,
    pub filled_ask_this_round: Uint128,
}

#[cw_serde]
pub struct Executor {
    pub address: CanonicalAddr,
    pub reward_assets: [Asset; 2],
}

impl Order {
    // create new order given a price and an offer amount
    pub fn new(
        order_id: u64,
        bidder_addr: CanonicalAddr,
        direction: OrderDirection,
        price: Decimal,
        ask_amount: Uint128,
    ) -> Self {
        let offer_amount = match direction {
            OrderDirection::Buy => ask_amount * price,
            OrderDirection::Sell => ask_amount * (Decimal::one() / price),
        };

        Order {
            direction,
            order_id,
            bidder_addr,
            offer_amount,
            ask_amount,
            filled_offer_amount: Uint128::zero(),
            filled_ask_amount: Uint128::zero(),
            status: OrderStatus::Open,
        }
    }

    // The price will be calculated by the number of base coins divided by the number of quote coins
    pub fn get_price(&self) -> Decimal {
        match self.direction {
            OrderDirection::Buy => Decimal::from_ratio(self.offer_amount, self.ask_amount),
            OrderDirection::Sell => Decimal::from_ratio(self.ask_amount, self.offer_amount),
        }
    }

    pub fn to_response(
        &self,
        api: &dyn Api,
        base_info: AssetInfo,
        quote_info: AssetInfo,
    ) -> StdResult<OrderResponse> {
        Ok(OrderResponse {
            order_id: self.order_id,
            status: self.status,
            direction: self.direction,
            bidder_addr: api.addr_humanize(&self.bidder_addr)?.to_string(),
            offer_asset: Asset {
                amount: self.offer_amount,
                info: match self.direction {
                    OrderDirection::Buy => quote_info.clone(),
                    OrderDirection::Sell => base_info.clone(),
                },
            },
            ask_asset: Asset {
                amount: self.ask_amount,
                info: match self.direction {
                    OrderDirection::Buy => base_info.clone(),
                    OrderDirection::Sell => quote_info.clone(),
                },
            },
            filled_offer_amount: self.filled_offer_amount,
            filled_ask_amount: self.filled_ask_amount,
        })
    }
}

impl OrderWithFee {
    pub fn from_order(order: Order) -> Self {
        Self {
            order_id: order.order_id,
            status: order.status,
            direction: order.direction,
            bidder_addr: order.bidder_addr,
            offer_amount: order.offer_amount,
            ask_amount: order.ask_amount,
            filled_offer_amount: order.filled_offer_amount,
            filled_ask_amount: order.filled_ask_amount,
            reward_fee: Uint128::zero(),
            filled_ask_this_round: Uint128::zero(),
            filled_offer_this_round: Uint128::zero(),
        }
    }

    pub fn from_orders(orders: Vec<Order>) -> Vec<Self> {
        orders.into_iter().map(Self::from_order).collect()
    }
    // create new order given a price and an offer amount
    pub fn fill_order(
        &mut self,
        ask_amount: Uint128,
        offer_amount: Uint128,
        min_ask_to_fulfilled: Uint128,
        min_offer_to_fulfilled: Uint128,
    ) -> StdResult<()> {
        self.filled_ask_amount += ask_amount;
        self.filled_offer_amount += offer_amount;

        self.filled_ask_this_round = ask_amount;
        self.filled_offer_this_round = offer_amount;

        if self.offer_amount.checked_sub(self.filled_offer_amount)? < min_offer_to_fulfilled
            || self.ask_amount.checked_sub(self.filled_ask_amount)? < min_ask_to_fulfilled
        {
            self.status = OrderStatus::Fulfilled;
        } else {
            self.status = OrderStatus::PartialFilled;
        }
        Ok(())
    }

    pub fn match_order(
        &mut self,
        storage: &mut dyn Storage,
        pair_key: &[u8],
        refund_threshold: Uint128,
    ) -> StdResult<Uint128> {
        let order = Order {
            order_id: self.order_id,
            status: self.status,
            direction: self.direction,
            bidder_addr: self.bidder_addr.to_owned(),
            offer_amount: self.offer_amount,
            ask_amount: self.ask_amount,
            filled_offer_amount: self.filled_offer_amount,
            filled_ask_amount: self.filled_ask_amount,
        };
        if self.status == OrderStatus::Fulfilled {
            // When status is Fulfilled, remove order and refunds offer amount
            let remaining: Uint128 = order.offer_amount - order.filled_offer_amount;
            // check refunds amount is less than minimum quote refund amount
            let min_offer_refund = match order.direction {
                OrderDirection::Buy => refund_threshold,
                OrderDirection::Sell => {
                    refund_threshold * Decimal::one().atomics() / order.get_price().atomics()
                }
            };

            let refunds_amount = if remaining >= min_offer_refund {
                remaining
            } else {
                Uint128::zero()
            };
            remove_order(storage, pair_key, &order)?;
            Ok(refunds_amount)
        } else {
            // update order
            store_order(storage, pair_key, &order, false)?;
            Ok(Uint128::zero())
        }
    }

    pub fn is_fulfilled(&self) -> bool {
        self.offer_amount < self.filled_offer_amount + Uint128::from(MIN_VOLUME)
            || self.ask_amount < self.filled_ask_amount + Uint128::from(MIN_VOLUME)
    }

    pub fn get_price(&self) -> Decimal {
        match self.direction {
            OrderDirection::Buy => Decimal::from_ratio(self.offer_amount, self.ask_amount),
            OrderDirection::Sell => Decimal::from_ratio(self.ask_amount, self.offer_amount),
        }
    }
}

/// Ticks are stored in Ordered database, so we just need to process at 50 recent ticks is ok
#[cw_serde]
pub struct OrderBook {
    pub base_coin_info: AssetInfoRaw,
    pub quote_coin_info: AssetInfoRaw,
    pub spread: Option<Decimal>,
    pub min_quote_coin_amount: Uint128,
    pub refund_threshold: Option<Uint128>,
    pub min_offer_to_fulfilled: Option<Uint128>,
    pub min_ask_to_fulfilled: Option<Uint128>,
}

impl OrderBook {
    pub fn new(
        base_coin_info: AssetInfoRaw,
        quote_coin_info: AssetInfoRaw,
        spread: Option<Decimal>,
    ) -> Self {
        OrderBook {
            base_coin_info,
            quote_coin_info,
            spread,
            min_quote_coin_amount: Uint128::zero(),
            refund_threshold: None,
            min_offer_to_fulfilled: None,
            min_ask_to_fulfilled: None,
        }
    }

    pub fn to_response(&self, api: &dyn Api) -> StdResult<OrderBookResponse> {
        Ok(OrderBookResponse {
            base_coin_info: self.base_coin_info.to_normal(api)?,
            quote_coin_info: self.quote_coin_info.to_normal(api)?,
            spread: self.spread,
            min_quote_coin_amount: self.min_quote_coin_amount,
            refund_threshold: self
                .refund_threshold
                .unwrap_or(Uint128::from(REFUNDS_THRESHOLD)),
            min_offer_to_fulfilled: self
                .min_offer_to_fulfilled
                .unwrap_or(Uint128::from(MIN_VOLUME)),
            min_ask_to_fulfilled: self
                .min_ask_to_fulfilled
                .unwrap_or(Uint128::from(MIN_VOLUME)),
        })
    }

    pub fn get_pair_key(&self) -> Vec<u8> {
        pair_key_from_asset_keys(
            self.base_coin_info.as_bytes(),
            self.quote_coin_info.as_bytes(),
        )
    }

    pub fn add_order(&mut self, storage: &mut dyn Storage, order: &Order) -> StdResult<u64> {
        let pair_key = &self.get_pair_key();
        store_order(storage, pair_key, order, true)
    }

    fn best_price(
        &self,
        storage: &dyn Storage,
        direction: OrderDirection,
        price_increasing: OrderBy,
    ) -> Option<(Decimal, u64)> {
        let pair_key = &self.get_pair_key();
        // get last tick if price_increasing is true, otherwise get first tick
        let tick_namespaces = &[PREFIX_TICK, pair_key, direction.as_bytes()];
        let position_bucket = ReadonlyBucket::multilevel(storage, tick_namespaces);

        if let Some(Ok((price_key, total_orders))) =
            position_bucket.range(None, None, price_increasing).next()
        {
            // price is rounded already
            let price = Decimal::raw(u128::from_be_bytes(price_key.try_into().unwrap()));
            return Some((price, total_orders));
        }

        None
    }

    pub fn highest_price(
        &self,
        storage: &dyn Storage,
        direction: OrderDirection,
    ) -> Option<(Decimal, u64)> {
        self.best_price(storage, direction, OrderBy::Descending)
    }

    pub fn lowest_price(
        &self,
        storage: &dyn Storage,
        direction: OrderDirection,
    ) -> Option<(Decimal, u64)> {
        self.best_price(storage, direction, OrderBy::Ascending)
    }

    pub fn orders_at(
        &self,
        storage: &dyn Storage,
        price: Decimal,
        direction: OrderDirection,
        start_after: Option<u64>,
        limit: Option<u32>,
    ) -> Option<Vec<Order>> {
        let pair_key = &self.get_pair_key();
        read_orders_with_indexer::<OrderDirection>(
            storage,
            &[
                PREFIX_ORDER_BY_PRICE,
                pair_key,
                &price.atomics().to_be_bytes(),
            ],
            Box::new(move |item| direction.eq(item)),
            start_after,
            limit,
            Some(OrderBy::Ascending), // first in first out
        )
        .unwrap()
    }

    // get_orders returns all orders in the order book, with pagination
    pub fn get_orders(
        &self,
        storage: &dyn Storage,
        start_after: Option<u64>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    ) -> StdResult<Vec<Order>> {
        let pair_key = &self.get_pair_key();
        read_orders(storage, pair_key, start_after, limit, order_by)
    }

    /// find list best buy / sell prices
    pub fn find_list_match_price(
        &self,
        storage: &dyn Storage,
        limit: Option<u32>,
    ) -> Option<(Vec<Decimal>, Vec<Decimal>)> {
        let pair_key = &self.get_pair_key();
        // asc
        let sell_price_list = query_ticks_prices(
            storage,
            pair_key,
            OrderDirection::Sell,
            None,
            limit,
            Some(OrderBy::Ascending),
        );
        // guard code
        if sell_price_list.is_empty() {
            return None;
        }

        let start_after = Decimal::from_atomics(
            sell_price_list[0]
                .atomics()
                .checked_sub(Uint128::from(1u64))
                .unwrap_or_default(), // sub 1 because we want to get buy price at the smallest sell price as well, not skip it
            Decimal::DECIMAL_PLACES,
        )
        .ok();
        // desc, all items in this list are ge than the first item in sell list
        let best_buy_price_list = query_ticks_prices_with_end(
            storage,
            pair_key,
            OrderDirection::Buy,
            None,
            start_after,
            limit,
            Some(OrderBy::Descending),
        );
        // both price lists are applicable because buy list is always larger than the first item of sell list
        let best_sell_price_list = sell_price_list;
        if best_buy_price_list.is_empty() || best_sell_price_list.is_empty() {
            return None;
        }
        Some((best_buy_price_list, best_sell_price_list))
    }

    /// matches orders sequentially, starting from buy orders with the highest price, and sell orders with the lowest price
    /// The matching continues until there's no more matchable orders.
    pub fn query_orders_by_price_and_direction(
        &self,
        storage: &dyn Storage,
        price: Decimal,
        direction: OrderDirection,
        limit: Option<u32>,
    ) -> Option<Vec<Order>> {
        let pair_key = &self.get_pair_key();
        let price_key = price.atomics().to_be_bytes();

        // there is a limit, and we just match a batch with maximum orders reach the limit step by step
        read_orders_with_indexer::<OrderDirection>(
            storage,
            &[PREFIX_ORDER_BY_PRICE, pair_key, &price_key],
            Box::new(move |x| direction.eq(x)),
            None,
            limit,
            Some(OrderBy::Ascending), // if mean we process from first to last order in the orderlist
        )
        .unwrap_or_default()
    }
}

impl Executor {
    pub fn new(address: CanonicalAddr, reward_assets: [Asset; 2]) -> Self {
        Executor {
            address,
            reward_assets,
        }
    }
}
