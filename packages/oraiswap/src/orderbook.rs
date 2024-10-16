use crate::asset::{Asset, AssetInfo};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, CanonicalAddr, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct ContractInfo {
    pub name: String,
    pub version: String,
    // admin can update the parameter, may be multisig
    pub admin: CanonicalAddr,
    pub commission_rate: String,
    pub reward_address: CanonicalAddr,
    pub operator: Option<CanonicalAddr>,
    #[serde(default)]
    pub is_paused: bool,
}

#[cw_serde]
pub enum OrderType {
    Limit,
    Market,
}

impl OrderType {
    pub fn is_limit(&self) -> bool {
        matches!(self, OrderType::Limit)
    }

    pub fn is_market(&self) -> bool {
        !self.is_limit()
    }
}

#[cw_serde]
#[derive(Copy, Default)]
pub enum OrderDirection {
    #[default]
    Buy,
    Sell,
}

impl OrderDirection {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            OrderDirection::Buy => &[0u8],
            OrderDirection::Sell => &[1u8],
        }
    }
}

#[cw_serde]
#[derive(Copy)]
pub enum OrderStatus {
    Open,
    PartialFilled,
    Fulfilled,
    Cancel,
}

impl OrderStatus {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            OrderStatus::Open => &[0u8],
            OrderStatus::PartialFilled => &[1u8],
            OrderStatus::Fulfilled => &[2u8],
            OrderStatus::Cancel => &[3u8],
        }
    }
}

#[cw_serde]
pub struct InstantiateMsg {
    pub name: Option<String>,
    pub version: Option<String>,
    pub admin: Option<String>,
    pub commission_rate: Option<String>,
    pub reward_address: String,
    pub operator: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),

    Pause {},
    Unpause {},
    UpdateAdmin {
        admin: Addr,
    },

    UpdateConfig {
        reward_address: Option<Addr>,
        commission_rate: Option<String>,
    },

    UpdateOperator {
        operator: Option<String>,
    },

    CreateOrderBookPair {
        base_coin_info: AssetInfo,
        quote_coin_info: AssetInfo,
        spread: Option<Decimal>,
        min_quote_coin_amount: Uint128,
        refund_threshold: Option<Uint128>,
        min_offer_to_fulfilled: Option<Uint128>,
        min_ask_to_fulfilled: Option<Uint128>,
    },

    UpdateOrderBookPair {
        asset_infos: [AssetInfo; 2],
        spread: Option<Decimal>,
        min_quote_coin_amount: Option<Uint128>,
        refund_threshold: Option<Uint128>,
        min_offer_to_fulfilled: Option<Uint128>,
        min_ask_to_fulfilled: Option<Uint128>,
    },

    ///////////////////////
    /// User Operations ///
    ///////////////////////
    SubmitOrder {
        direction: OrderDirection, // default is buy, with sell then it is reversed
        assets: [Asset; 2],
    },

    // ///////////////////////
    // /// User Operations ///
    // ///////////////////////
    SubmitMarketOrder {
        direction: OrderDirection, // default is buy, with sell then it is reversed
        asset_infos: [AssetInfo; 2],
        slippage: Option<Decimal>,
    },
    CancelOrder {
        order_id: u64,
        asset_infos: [AssetInfo; 2],
    },

    /// Arbitrager remove order book
    RemoveOrderBookPair {
        asset_infos: [AssetInfo; 2],
    },

    WithdrawToken {
        asset: Asset,
    },
    WhitelistTrader {
        trader: Addr,
    },
    RemoveTrader {
        trader: Addr,
    },
}

#[cw_serde]
pub enum Cw20HookMsg {
    SubmitOrder {
        direction: OrderDirection,
        assets: [Asset; 2],
    },
    SubmitMarketOrder {
        direction: OrderDirection, // default is buy, with sell then it is reversed
        asset_infos: [AssetInfo; 2],
        slippage: Option<Decimal>,
    },
}

#[cw_serde]
pub enum OrderFilter {
    Bidder(String), // filter by bidder
    Price(Decimal), // filter by price
    Tick,           // filter by direction
    None,           // no filter
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ContractInfoResponse)]
    ContractInfo {},
    #[returns(OrderBookResponse)]
    OrderBook { asset_infos: [AssetInfo; 2] },
    #[returns(OrderBooksResponse)]
    OrderBooks {
        start_after: Option<Vec<u8>>,
        limit: Option<u32>,
        order_by: Option<i32>, // convert OrderBy to i32
    },
    #[returns(OrderResponse)]
    Order {
        order_id: u64,
        asset_infos: [AssetInfo; 2],
    },
    #[returns(OrdersResponse)]
    Orders {
        asset_infos: [AssetInfo; 2],
        filter: OrderFilter,
        direction: Option<OrderDirection>,
        start_after: Option<u64>,
        limit: Option<u32>,
        order_by: Option<i32>, // convert OrderBy to i32
    },
    #[returns(TickResponse)]
    Tick {
        price: Decimal,
        asset_infos: [AssetInfo; 2],
        direction: OrderDirection,
    },
    #[returns(TicksResponse)]
    Ticks {
        asset_infos: [AssetInfo; 2],
        direction: OrderDirection,
        start_after: Option<Decimal>,
        end: Option<Decimal>,
        limit: Option<u32>,
        order_by: Option<i32>, // convert OrderBy to i32
    },
    #[returns(LastOrderIdResponse)]
    LastOrderId {},
    #[returns(Decimal)]
    MidPrice { asset_infos: [AssetInfo; 2] },
    #[returns(SimulateMarketOrderResponse)]
    SimulateMarketOrder {
        direction: OrderDirection, // default is buy, with sell then it is reversed
        asset_infos: [AssetInfo; 2],
        slippage: Option<Decimal>,
        offer_amount: Uint128,
    },
    #[returns(Vec<String>)]
    WhitelistedTraders {},
}

#[cw_serde]
pub struct ContractInfoResponse {
    pub name: String,
    pub version: String,

    // admin can update the parameter, may be multisig
    pub admin: Addr,
    pub commission_rate: String,
    pub reward_address: Addr,
    pub operator: Option<Addr>,
    pub is_paused: bool,
}

#[cw_serde]
pub struct OrderResponse {
    pub order_id: u64,
    pub status: OrderStatus,
    pub direction: OrderDirection,
    pub bidder_addr: String,
    pub offer_asset: Asset,
    pub ask_asset: Asset,
    pub filled_offer_amount: Uint128,
    pub filled_ask_amount: Uint128,
}

#[cw_serde]
pub struct OrderBookResponse {
    pub base_coin_info: AssetInfo,
    pub quote_coin_info: AssetInfo,
    pub spread: Option<Decimal>,
    pub min_quote_coin_amount: Uint128,
    pub refund_threshold: Uint128,
    pub min_offer_to_fulfilled: Uint128,
    pub min_ask_to_fulfilled: Uint128,
}

#[cw_serde]
pub struct OrderBooksResponse {
    pub order_books: Vec<OrderBookResponse>,
}

#[cw_serde]
pub struct OrdersResponse {
    pub orders: Vec<OrderResponse>,
}

#[cw_serde]
pub struct TickResponse {
    pub price: Decimal,
    pub total_orders: u64,
}

#[cw_serde]
pub struct TicksResponse {
    pub ticks: Vec<TickResponse>,
}

#[cw_serde]
pub struct LastOrderIdResponse {
    pub last_order_id: u64,
}

#[cw_serde]
pub struct SimulateMarketOrderResponse {
    pub receive: Uint128,
    pub refunds: Uint128,
}

#[cw_serde]
pub struct Payment {
    pub address: Addr,
    pub asset: Asset,
}

/// We currently take no arguments for migrations
#[cw_serde]
pub struct MigrateMsg {}
