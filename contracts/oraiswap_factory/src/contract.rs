use std::convert::TryFrom;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    to_json_binary, Addr, Binary, CanonicalAddr, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Reply, Response, StdError, StdResult, SubMsg, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use oraiswap::error::ContractError;
use oraiswap::querier::query_pair_info_from_pair;
use oraiswap::response::MsgInstantiateContractResponse;

use crate::state::{read_pairs, Config, CONFIG, PAIRS};

use oraiswap::asset::{pair_key, Asset, AssetInfo, PairInfo, PairInfoRaw};
use oraiswap::factory::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, PairsResponse, ProvideLiquidityParams,
    QueryMsg,
};
use oraiswap::pair::{
    InstantiateMsg as PairInstantiateMsg, DEFAULT_COMMISSION_RATE, DEFAULT_OPERATOR_FEE,
};

const INSTANTIATE_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        oracle_addr: deps.api.addr_canonicalize(msg.oracle_addr.as_str())?,
        owner: deps.api.addr_canonicalize(info.sender.as_str())?,
        token_code_id: msg.token_code_id,
        pair_code_id: msg.pair_code_id,
        commission_rate: msg
            .commission_rate
            .unwrap_or(DEFAULT_COMMISSION_RATE.to_string()),
        operator_fee: msg.operator_fee.unwrap_or(DEFAULT_OPERATOR_FEE.to_string()),
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            owner,
            token_code_id,
            pair_code_id,
        } => execute_update_config(deps, env, info, owner, token_code_id, pair_code_id),
        ExecuteMsg::CreatePair {
            asset_infos,
            pair_admin,
            operator,
            provide_liquidity,
        } => execute_create_pair(
            deps,
            env,
            info,
            asset_infos,
            pair_admin,
            operator,
            provide_liquidity,
        ),
        ExecuteMsg::AddPair { pair_info } => execute_add_pair_manually(deps, env, info, pair_info),
        ExecuteMsg::MigrateContract {
            contract_addr,
            new_code_id,
            msg,
        } => migrate_pair(deps, env, info, contract_addr, new_code_id, msg),
        ExecuteMsg::ProvideLiquidity { assets, receiver } => {
            execute_provide_liquidity(deps, env, info, assets, receiver)
        }
    }
}

pub fn migrate_pair(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    contract_addr: String,
    new_code_id: u64,
    msg: Binary,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // permission check
    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let wasm_msg = WasmMsg::Migrate {
        contract_addr,
        new_code_id,
        msg,
    };
    Ok(Response::new()
        .add_attribute("action", "migrate_factory_contract")
        .add_message(wasm_msg))
}

// Only owner can execute it
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    token_code_id: Option<u64>,
    pair_code_id: Option<u64>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // permission check
    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    if let Some(token_code_id) = token_code_id {
        config.token_code_id = token_code_id;
    }

    if let Some(pair_code_id) = pair_code_id {
        config.pair_code_id = pair_code_id;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

// Anyone can execute it to create swap pair
pub fn execute_create_pair(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_infos: [AssetInfo; 2],
    pair_admin: Option<String>,
    operator: Option<String>,
    provide_liquidity: Option<ProvideLiquidityParams>,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    let raw_infos: [oraiswap::asset::AssetInfoRaw; 2] = [
        asset_infos[0].to_raw(deps.api)?,
        asset_infos[1].to_raw(deps.api)?,
    ];

    let pair_key = pair_key(&raw_infos);

    // can not update pair once updated
    if let Ok(Some(_)) = PAIRS.may_load(deps.storage, &pair_key) {
        return Err(ContractError::PairExisted {});
    }

    PAIRS.save(
        deps.storage,
        &pair_key,
        &PairInfoRaw {
            oracle_addr: config.oracle_addr.clone(),
            liquidity_token: CanonicalAddr::from(vec![]),
            contract_addr: CanonicalAddr::from(vec![]),
            asset_infos: raw_infos,
            commission_rate: config.commission_rate.clone(),
            operator_fee: config.operator_fee.clone(),
        },
    )?;
    let pair_admin = pair_admin.unwrap_or(env.contract.address.to_string());

    let operator_addr = operator.map(|op| deps.api.addr_validate(&op)).transpose()?;

    // if provide_liquidity is not None, transfer all cw20 tokens to this contract
    let mut messages: Vec<CosmosMsg> = vec![];

    if let Some(ProvideLiquidityParams { assets, receiver }) = provide_liquidity {
        let receiver = receiver.unwrap_or(info.sender.clone());
        for asset in &assets {
            // If the pool is token contract, then we need to execute TransferFrom msg to receive funds
            if let AssetInfo::Token { contract_addr, .. } = &asset.info {
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount: asset.amount,
                    })?,
                    funds: vec![],
                }));
            }
        }

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_json_binary(&ExecuteMsg::ProvideLiquidity { assets, receiver })?,
            funds: info.funds,
        }));
    }

    Ok(Response::new()
        .add_submessage(SubMsg::reply_on_success(
            WasmMsg::Instantiate {
                code_id: config.pair_code_id,
                funds: vec![],
                admin: Some(pair_admin.clone()),
                label: "pair".to_string(),
                msg: to_json_binary(&PairInstantiateMsg {
                    oracle_addr: deps.api.addr_humanize(&config.oracle_addr)?,
                    asset_infos: asset_infos.clone(),
                    token_code_id: config.token_code_id,
                    commission_rate: Some(config.commission_rate),
                    admin: Some(deps.api.addr_validate(&pair_admin)?),
                    operator_fee: Some(config.operator_fee),
                    operator: operator_addr,
                })?,
            },
            INSTANTIATE_REPLY_ID,
        ))
        .add_attributes(vec![
            ("action", "create_pair"),
            ("pair", &format!("{}-{}", asset_infos[0], asset_infos[1])),
        ])
        .add_messages(messages))
}

// Only owner can execute it
pub fn execute_add_pair_manually(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pair_info: PairInfo,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    // permission check
    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let raw_infos = [
        pair_info.asset_infos[0].to_raw(deps.api)?,
        pair_info.asset_infos[1].to_raw(deps.api)?,
    ];

    let pair_key = pair_key(&raw_infos);

    // can not update pair once updated
    if let Ok(Some(_)) = PAIRS.may_load(deps.storage, &pair_key) {
        return Err(ContractError::PairExisted {});
    }

    PAIRS.save(
        deps.storage,
        &pair_key,
        &PairInfoRaw {
            oracle_addr: deps.api.addr_canonicalize(pair_info.oracle_addr.as_str())?,
            liquidity_token: deps
                .api
                .addr_canonicalize(pair_info.liquidity_token.as_str())?,
            contract_addr: deps
                .api
                .addr_canonicalize(pair_info.contract_addr.as_str())?,
            asset_infos: raw_infos,
            commission_rate: pair_info.commission_rate.clone(),
            operator_fee: pair_info.operator_fee,
        },
    )?;

    Ok(Response::new().add_attributes(vec![
        ("action", "add_pair"),
        (
            "pair",
            &format!("{}-{}", pair_info.asset_infos[0], pair_info.asset_infos[1]),
        ),
    ]))
}

pub fn execute_provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: [Asset; 2],
    receiver: Addr,
) -> Result<Response, ContractError> {
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    let asset_infos = [assets[0].info.clone(), assets[1].info.clone()];
    let pair_key = pair_key(&asset_infos.map(|a| a.to_raw(deps.api).unwrap()));
    let pair_raw = PAIRS.load(deps.storage, &pair_key)?;
    let pair_contract = deps.api.addr_humanize(&pair_raw.contract_addr)?;

    // Transfer native asset to pair contract
    let mut funds: Vec<Coin> = vec![];
    let mut cw20_msgs: Vec<CosmosMsg> = vec![];
    for (_i, asset) in assets.iter().enumerate() {
        match &asset.info {
            AssetInfo::NativeToken { denom } => {
                funds.push(Coin {
                    denom: denom.clone(),
                    amount: asset.amount,
                });
            }
            AssetInfo::Token { contract_addr, .. } => {
                cw20_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_owned().into(),
                    msg: to_json_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                        spender: pair_contract.to_string(),
                        amount: asset.amount,
                        expires: None,
                    })?,
                    funds: vec![],
                }));
            }
        }
    }

    // Execute provide liquidity
    let provide_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_contract.to_string(),
        msg: to_json_binary(&oraiswap::pair::ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance: None,
            receiver: Some(receiver),
        })?,
        funds,
    });

    Ok(Response::new()
        .add_messages(cw20_msgs)
        .add_message(provide_msg))
}

/// This just stores the result for future query
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let data = msg.result.unwrap().data.unwrap();
    let res = MsgInstantiateContractResponse::try_from(data.as_slice()).map_err(|_| {
        StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
    })?;

    let pair_contract = Addr::unchecked(res.address);
    let pair_info = query_pair_info_from_pair(&deps.querier, pair_contract.clone())?;
    let pair_key = pair_key(&pair_info.asset_infos.map(|a| a.to_raw(deps.api).unwrap()));

    // get pair info raw from state
    let mut pair_info_raw = PAIRS.load(deps.storage, &pair_key)?;

    // make sure creator can update their pairs
    if !pair_info_raw.contract_addr.is_empty() {
        return Err(ContractError::PairRegistered {});
    }

    // the contract must follow the standard interface
    pair_info_raw.liquidity_token = deps
        .api
        .addr_canonicalize(pair_info.liquidity_token.as_str())?;
    pair_info_raw.contract_addr = deps.api.addr_canonicalize(pair_contract.as_str())?;

    PAIRS.save(deps.storage, &pair_key, &pair_info_raw)?;

    Ok(Response::new().add_attributes(vec![
        ("pair_contract_address", pair_contract.as_str()),
        ("liquidity_token_addr", pair_info.liquidity_token.as_str()),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::Pair { asset_infos } => to_json_binary(&query_pair(deps, asset_infos)?),
        QueryMsg::Pairs { start_after, limit } => {
            to_json_binary(&query_pairs(deps, start_after, limit)?)
        }
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let state: Config = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        oracle_addr: deps.api.addr_humanize(&state.oracle_addr)?,
        owner: deps.api.addr_humanize(&state.owner)?,
        token_code_id: state.token_code_id,
        pair_code_id: state.pair_code_id,
    };

    Ok(resp)
}

pub fn query_pair(deps: Deps, asset_infos: [AssetInfo; 2]) -> StdResult<PairInfo> {
    let pair_key = pair_key(&[
        asset_infos[0].to_raw(deps.api)?,
        asset_infos[1].to_raw(deps.api)?,
    ]);
    let pair_info: PairInfoRaw = PAIRS.load(deps.storage, &pair_key)?;
    pair_info.to_normal(deps.api)
}

pub fn query_pairs(
    deps: Deps,
    start_after: Option<[AssetInfo; 2]>,
    limit: Option<u32>,
) -> StdResult<PairsResponse> {
    let start_after = if let Some(start_after) = start_after {
        Some([
            start_after[0].to_raw(deps.api)?,
            start_after[1].to_raw(deps.api)?,
        ])
    } else {
        None
    };

    let pairs: Vec<PairInfo> = read_pairs(deps.storage, deps.api, start_after, limit)?;
    let resp = PairsResponse { pairs };

    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
