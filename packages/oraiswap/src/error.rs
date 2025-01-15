use cosmwasm_std::{Decimal, OverflowError, StdError, Uint128};
use cw_utils::PaymentError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Max spread assertion")]
    MaxSpreadAssertion {},

    #[error("Max slippage assertion")]
    MaxSlippageAssertion {},

    #[error("Slippage_tolerance cannot bigger than 1")]
    InvalidExceedOneSlippage {},

    #[error("Withdraw amount is too small compared to the total share")]
    InvalidZeroRatio {},

    #[error("Asset mismatch")]
    AssetMismatch {},

    #[error("Offer pool is zero")]
    OfferPoolIsZero {},

    #[error("Pair already exists")]
    PairExisted {},

    #[error("Pair was already registered")]
    PairRegistered {},

    #[error(
        "Assertion failed; minimum receive amount: {minium_receive}, swap amount: {swap_amount}"
    )]
    SwapAssertionFailure {
        minium_receive: Uint128,
        swap_amount: Uint128,
    },

    #[error("must provide operations")]
    NoSwapOperation {},

    #[error("invalid cw20 hook message")]
    InvalidCw20HookMessage {},

    #[error("must provide native token")]
    MustProvideNativeToken {}, // only allowing buy token and sell token with native token

    #[error("Order book pair already exists")]
    OrderBookAlreadyExists {},

    #[error("Order asset must not be zero")]
    AssetMustNotBeZero {},

    #[error("Order {order_id} has already fulfilled")]
    OrderFulfilled { order_id: u64 },

    #[error("Amount of {quote_coin} must be greater than {min_quote_amount}")]
    TooSmallQuoteAsset {
        quote_coin: String,
        min_quote_amount: Uint128,
    },

    #[error("Price {price} must not be zero")]
    PriceMustNotBeZero { price: Decimal },

    #[error("Offer amount {offer_amount} is too small")]
    OfferAmountTooSmall { offer_amount: Uint128 },

    #[error("Slippage {slippage} must be less than one")]
    SlippageMustLessThanOne { slippage: Decimal },

    #[error("Unable to find market order")]
    UnableToFindMarketOrder {},

    #[error("Unable to excute matching orders")]
    UnableToExecuteMatching {},

    #[error("The contract upgrading process has not completed yet. Please come back after a while, thank you for your patience!")]
    ContractUpgrade {},

    #[error("This pool is not open to everyone, only whitelisted traders can swap")]
    PoolWhitelisted {},

    #[error("Cannot find a matched price")]
    NoMatchedPrice {},

    #[error("Price cannot be greater than {price}")]
    PriceNotGreaterThan { price: Decimal },

    #[error("Price cannot be less than {price}")]
    PriceNotLessThan { price: Decimal },

    #[error("Cannot create market order")]
    CannotCreateMarketOrder {},

    #[error("Invalid funds")]
    InvalidFunds {},

    #[error("Contract paused")]
    Paused {},

    #[error("Restricted prefix existed")]
    RestrictPrefixExisted {},

    #[error("Creator is whitelisted already")]
    CreatorAlreadyExists {},

    #[error("Not found this creator")]
    CreatorNotFound {}
}
