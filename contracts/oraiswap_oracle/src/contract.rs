use std::ops::Mul;

use cosmwasm_std::{
    to_binary, Binary, Coin, Decimal, Deps, DepsMut, Env, HandleResponse, HumanAddr, InitResponse,
    MessageInfo, StdResult, Uint128,
};

use oraiswap::oracle::{
    ContractInfo, ContractInfoResponse, ExchangeRateItem, ExchangeRateResponse,
    ExchangeRatesResponse, OracleContractMsg, OracleContractQuery, OracleExchangeMsg,
    OracleExchangeQuery, OracleMarketMsg, OracleMarketQuery, OracleMsg, OracleQuery,
    OracleTreasuryQuery, SwapResponse, TaxCapResponse, TaxRateResponse,
};

use oraiswap::error::ContractError;
use oraiswap::oracle::InitMsg;
// use crate::msg::{HandleMsg, InitMsg};
use crate::state::{CONTRACT_INFO, EXCHANGE_RATES, TAX_CAP, TAX_RATE};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:oraiswap_oracle";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// 10^18 is maximum decimal that we support
const DECIMAL_FRACTIONAL: Uint128 = Uint128(1_000_000_000_000_000_000);

pub fn init(
    deps: DepsMut,
    _env: Env,
    msg_info: MessageInfo,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let creator = deps.api.canonical_address(&msg_info.sender)?;
    let info = ContractInfo {
        name: msg.name.unwrap_or(CONTRACT_NAME.to_string()),
        version: msg.version.unwrap_or(CONTRACT_VERSION.to_string()),
        creator: creator.clone(),
        // admin should be multisig
        admin: if let Some(admin) = msg.admin {
            deps.api.canonical_address(&admin)?
        } else {
            creator
        },
    };
    CONTRACT_INFO.save(deps.storage, &info)?;

    // defaul is orai/orai 1:1 (no tax)
    EXCHANGE_RATES.save(deps.storage, b"orai", &Decimal::one())?;

    // return default
    Ok(InitResponse::default())
}

pub fn handle(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: OracleMsg,
) -> Result<HandleResponse, ContractError> {
    match msg {
        OracleMsg::Exchange(handle_data) => match handle_data {
            OracleExchangeMsg::UpdateExchangeRate {
                denom,
                exchange_rate,
            } => handle_update_exchange_rate(deps, info, denom, exchange_rate),
            OracleExchangeMsg::DeleteExchangeRate { denom } => {
                handle_delete_exchange_rate(deps, info, denom)
            }
        },
        OracleMsg::Market(handle_data) => match handle_data {
            OracleMarketMsg::Swap {
                offer_coin,
                ask_denom,
            } => handle_swap(deps, info, env.contract.address, offer_coin, ask_denom),
            OracleMarketMsg::SwapSend {
                to_address,
                offer_coin,
                ask_denom,
            } => handle_swap(deps, info, to_address, offer_coin, ask_denom),
        },
        OracleMsg::Contract(handle_data) => match handle_data {
            OracleContractMsg::UpdateAdmin { admin } => handle_update_admin(deps, info, admin),
        },
    }
}

pub fn handle_update_admin(
    deps: DepsMut,
    info: MessageInfo,
    admin: HumanAddr,
) -> Result<HandleResponse, ContractError> {
    let mut contract_info = CONTRACT_INFO.load(deps.storage)?;
    let sender_addr = deps.api.canonical_address(&info.sender)?;

    // check authorized
    if contract_info.admin.ne(&sender_addr) {
        return Err(ContractError::Unauthorized {});
    }

    // update new admin
    contract_info.admin = deps.api.canonical_address(&admin)?;
    CONTRACT_INFO.save(deps.storage, &contract_info)?;

    // return nothing new
    Ok(HandleResponse::default())
}

pub fn handle_update_exchange_rate(
    deps: DepsMut,
    info: MessageInfo,
    denom: String,
    exchange_rate: Decimal,
) -> Result<HandleResponse, ContractError> {
    let contract_info = CONTRACT_INFO.load(deps.storage)?;
    let sender_addr = deps.api.canonical_address(&info.sender)?;

    // check authorized
    if contract_info.admin.ne(&sender_addr) {
        return Err(ContractError::Unauthorized {});
    }

    EXCHANGE_RATES.save(deps.storage, denom.as_bytes(), &exchange_rate)?;

    Ok(HandleResponse::default())
}

pub fn handle_delete_exchange_rate(
    deps: DepsMut,
    info: MessageInfo,
    denom: String,
) -> Result<HandleResponse, ContractError> {
    let contract_info = CONTRACT_INFO.load(deps.storage)?;
    let sender_addr = deps.api.canonical_address(&info.sender)?;

    // check authorized
    if contract_info.admin.ne(&sender_addr) {
        return Err(ContractError::Unauthorized {});
    }

    EXCHANGE_RATES.remove(deps.storage, denom.as_bytes());

    Ok(HandleResponse::default())
}

// Only owner can execute it
pub fn handle_swap(
    deps: DepsMut,
    info: MessageInfo,
    to_address: HumanAddr,
    offer_coin: Coin,
    ask_denom: String,
) -> Result<HandleResponse, ContractError> {
    // TODO: implemented from here https://github.com/terra-money/core/blob/main/x/market/keeper/msg_server.go
    Ok(HandleResponse::default())
}

pub fn query(deps: Deps, env: Env, msg: OracleQuery) -> StdResult<Binary> {
    match msg {
        OracleQuery::Treasury(query_data) => match query_data {
            OracleTreasuryQuery::TaxRate {} => to_binary(&query_tax_rate(deps)?),
            OracleTreasuryQuery::TaxCap { denom } => to_binary(&query_tax_cap(deps, denom)?),
        },
        OracleQuery::Market(query_data) => match query_data {
            OracleMarketQuery::Swap {
                offer_coin,
                ask_denom,
            } => to_binary(&query_swap(deps, offer_coin, ask_denom)?),
        },
        OracleQuery::Exchange(query_data) => match query_data {
            OracleExchangeQuery::ExchangeRate {
                base_denom,
                quote_denom,
            } => to_binary(&query_exchange_rate(deps, base_denom, quote_denom)?),
            OracleExchangeQuery::ExchangeRates {
                base_denom,
                quote_denoms,
            } => to_binary(&query_exchange_rates(deps, base_denom, quote_denoms)?),
        },
        OracleQuery::Contract(query_data) => match query_data {
            OracleContractQuery::ContractInfo {} => to_binary(&query_contract_info(deps)?),
            OracleContractQuery::RewardPool { denom } => {
                to_binary(&query_contract_balance(deps, env, denom)?)
            }
        },
    }
}

pub fn query_tax_rate(deps: Deps) -> StdResult<TaxRateResponse> {
    // TODO : implemented here https://github.com/terra-money/core/tree/main/x/treasury/spec
    let rate = TAX_RATE.load(deps.storage)?;
    Ok(TaxRateResponse { rate })
}

pub fn query_tax_cap(deps: Deps, denom: String) -> StdResult<TaxCapResponse> {
    // TODO : implemented here https://github.com/terra-money/core/tree/main/x/treasury/spec
    let cap = TAX_CAP.load(deps.storage, denom.as_bytes())?;
    Ok(TaxCapResponse { cap })
}

pub fn query_swap(deps: Deps, offer_coin: Coin, ask_denom: String) -> StdResult<SwapResponse> {
    // TODO: implemented here https://github.com/terra-money/core/blob/main/x/market/keeper/querier.go
    // with offer_coin, ask for denom, will return receive, based on swap rate
    Ok(SwapResponse {
        receive: offer_coin.clone(),
    })
}

pub fn query_exchange_rate(
    deps: Deps,
    base_denom: String,
    quote_denom: String,
) -> StdResult<ExchangeRateResponse> {
    let base_rate = EXCHANGE_RATES
        .load(deps.storage, &base_denom.as_bytes())?
        .mul(DECIMAL_FRACTIONAL);
    let quote_rate = EXCHANGE_RATES
        .load(deps.storage, &quote_denom.as_bytes())?
        .mul(DECIMAL_FRACTIONAL);

    let exchange_rate = Decimal::from_ratio(quote_rate, base_rate);

    let res = ExchangeRateResponse {
        base_denom: base_denom.clone(),
        item: ExchangeRateItem {
            quote_denom,
            exchange_rate,
        },
    };

    Ok(res)
}

pub fn query_exchange_rates(
    deps: Deps,
    base_denom: String,
    quote_denoms: Vec<String>,
) -> StdResult<ExchangeRatesResponse> {
    let mut res = ExchangeRatesResponse {
        base_denom: base_denom.clone(),
        items: vec![],
    };

    let base_rate = EXCHANGE_RATES
        .load(deps.storage, &base_denom.as_bytes())?
        .mul(DECIMAL_FRACTIONAL);

    for quote_denom in quote_denoms {
        let quote_rate = EXCHANGE_RATES
            .load(deps.storage, &quote_denom.as_bytes())?
            .mul(DECIMAL_FRACTIONAL);

        let exchange_rate = Decimal::from_ratio(quote_rate, base_rate);

        res.items.push(ExchangeRateItem {
            quote_denom,
            exchange_rate,
        });
    }

    Ok(res)
}

pub fn query_contract_info(deps: Deps) -> StdResult<ContractInfoResponse> {
    let info = CONTRACT_INFO.load(deps.storage)?;
    Ok(ContractInfoResponse {
        version: info.version,
        name: info.name,
        admin: deps.api.human_address(&info.admin)?,
        creator: deps.api.human_address(&info.creator)?,
    })
}

pub fn query_contract_balance(deps: Deps, env: Env, denom: String) -> StdResult<Coin> {
    deps.querier.query_balance(env.contract.address, &denom)
}
