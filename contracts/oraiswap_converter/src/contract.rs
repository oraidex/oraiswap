use cosmwasm_std::{
    entry_point, from_json, to_json_binary, Addr, Attribute, BankMsg, Binary, Coin, CosmosMsg,
    Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use oraiswap::math::Converter128;

use crate::state::{
    read_config, read_token_ratio, store_config, store_token_ratio, token_ratio_remove, Config,
};

use oraiswap::converter::{
    ConfigResponse, ConvertInfoResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg,
    QueryMsg, TokenInfo, TokenRatio,
};

use oraiswap::asset::{Asset, AssetInfo};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    store_config(
        deps.storage,
        &Config {
            owner: deps.api.addr_canonicalize(info.sender.as_str())?,
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::UpdateConfig { owner } => update_config(deps, info, owner),
        ExecuteMsg::UpdatePair {
            from,
            to,
            is_mint_burn,
        } => update_pair(deps, info, from, to, is_mint_burn),
        ExecuteMsg::UnregisterPair { from } => unregister_pair(deps, info, from),
        ExecuteMsg::Convert {} => convert(deps, env, info),
        ExecuteMsg::ConvertReverse { from_asset } => convert_reverse(deps, env, info, from_asset),
        ExecuteMsg::WithdrawTokens { asset_infos } => withdraw_tokens(deps, env, info, asset_infos),
    }
}

pub fn update_config(deps: DepsMut, info: MessageInfo, owner: Addr) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage)?;

    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    config.owner = deps.api.addr_canonicalize(owner.as_str())?;

    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

pub fn receive_cw20(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_json(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Convert {}) => {
            // check permission
            let token_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
            let token_ratio = read_token_ratio(deps.storage, token_raw.as_slice())?;
            let amount = cw20_msg.amount * token_ratio.ratio;

            let message = process_build_convert_msg(
                token_ratio.info,
                amount,
                cw20_msg.sender,
                token_ratio.is_mint_burn,
            )?;

            Ok(Response::new().add_message(message).add_attributes(vec![
                ("action", "convert_token"),
                ("from_amount", &cw20_msg.amount.to_string()),
                ("to_amount", &amount.to_string()),
            ]))
        }
        Ok(Cw20HookMsg::ConvertReverse { from }) => {
            let asset_key = from.to_vec(deps.api)?;
            let token_ratio = read_token_ratio(deps.storage, &asset_key)?;

            if let AssetInfo::Token { contract_addr } = token_ratio.info.clone() {
                if contract_addr != info.sender {
                    return Err(StdError::generic_err("invalid cw20 hook message"));
                }

                let amount_receive = cw20_msg.amount.checked_div_decimal(token_ratio.ratio)?;

                let msgs = process_build_convert_reverse_msg(
                    deps.as_ref(),
                    Asset {
                        info: token_ratio.info,
                        amount: cw20_msg.amount,
                    },
                    Asset {
                        info: from,
                        amount: amount_receive,
                    },
                    deps.api.addr_validate(&cw20_msg.sender)?,
                    token_ratio.is_mint_burn,
                )?;

                Ok(Response::new().add_messages(msgs).add_attributes(vec![
                    ("action", "convert_token_reverse"),
                    ("from_amount", &cw20_msg.amount.to_string()),
                    ("to_amount", &amount_receive.to_string()),
                ]))
            } else {
                Err(StdError::generic_err("invalid cw20 hook message"))
            }
        }
        Err(_) => Err(StdError::generic_err("invalid cw20 hook message")),
    }
}

pub fn update_pair(
    deps: DepsMut,
    info: MessageInfo,
    from: TokenInfo,
    to: TokenInfo,
    is_mint_burn: bool,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let asset_key = from.info.to_vec(deps.api)?;

    // if is_mint_burn mechanism, check to_token must be cw20 token
    if is_mint_burn && to.info.is_native_token() {
        return Err(StdError::generic_err(
            "With mint_burn mechanism, to_token must be cw20 token",
        ));
    }

    let token_ratio = TokenRatio {
        info: to.info,
        ratio: Decimal::from_ratio(
            10u128.pow(to.decimals.into()),
            10u128.pow(from.decimals.into()),
        ),
        is_mint_burn,
    };

    store_token_ratio(deps.storage, &asset_key, &token_ratio)?;

    Ok(Response::new().add_attribute("action", "update_pair"))
}

pub fn unregister_pair(deps: DepsMut, info: MessageInfo, from: TokenInfo) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let asset_key = from.info.to_vec(deps.api)?;

    token_ratio_remove(deps.storage, &asset_key);

    Ok(Response::new().add_attribute("action", "unregister_convert_info"))
}

pub fn convert(deps: DepsMut, _env: Env, info: MessageInfo) -> StdResult<Response> {
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut attributes: Vec<Attribute> = vec![];
    attributes.push(("action", "convert_token").into());

    for native_coin in info.funds {
        let asset_key = native_coin.denom.as_bytes();
        let amount = native_coin.amount;
        attributes.push(("denom", native_coin.denom.clone()).into());
        attributes.push(("from_amount", amount.to_string()).into());
        let token_ratio = read_token_ratio(deps.storage, asset_key)?;
        let to_amount = amount * token_ratio.ratio;

        attributes.push(("to_amount", to_amount).into());

        messages.push(process_build_convert_msg(
            token_ratio.info,
            to_amount,
            info.sender.to_string(),
            token_ratio.is_mint_burn,
        )?)
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

pub fn convert_reverse(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    from_asset: AssetInfo,
) -> StdResult<Response> {
    let asset_key = from_asset.to_vec(deps.api)?;
    let token_ratio = read_token_ratio(deps.storage, &asset_key)?;

    if let AssetInfo::NativeToken { denom } = token_ratio.info.clone() {
        //check funds includes To token
        if let Some(native_coin) = info.funds.iter().find(|a| a.denom.eq(&denom)) {
            let amount_receive = native_coin.amount.checked_div_decimal(token_ratio.ratio)?;

            // dont care about mint burn because the sent info must be native -> cannot mint burn
            let message = Asset {
                info: from_asset,
                amount: amount_receive,
            }
            .into_msg(None, &deps.querier, info.sender.clone())?;

            Ok(Response::new().add_message(message).add_attributes(vec![
                ("action", "convert_token_reverse"),
                ("denom", native_coin.denom.as_str()),
                ("from_amount", &native_coin.amount.to_string()),
                ("to_amount", &amount_receive.to_string()),
            ]))
        } else {
            Err(StdError::generic_err("Cannot find the native token that matches the input to convert in convert_reverse()"))
        }
    } else {
        Err(StdError::generic_err("invalid cw20 hook message"))
    }
}

fn process_build_convert_msg(
    to_token: AssetInfo,
    amount: Uint128,
    recipient: String,
    is_mint_burn: bool,
) -> StdResult<CosmosMsg> {
    match to_token {
        AssetInfo::NativeToken { denom } => {
            if is_mint_burn {
                return Err(StdError::generic_err(
                    "With mint_burn mechanism, to_token must be cw20 token",
                ));
            }
            Ok(CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient,
                amount: vec![Coin { amount, denom }],
            }))
        }
        AssetInfo::Token { contract_addr } => {
            let cw20_msg = if is_mint_burn {
                Cw20ExecuteMsg::Mint { recipient, amount }
            } else {
                Cw20ExecuteMsg::Transfer { recipient, amount }
            };
            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_json_binary(&cw20_msg).unwrap(),
                funds: vec![],
            }))
        }
    }
}

fn process_build_convert_reverse_msg(
    deps: Deps,
    from_asset: Asset,
    to_asset: Asset,
    recipient: Addr,
    is_mint_burn: bool,
) -> StdResult<Vec<CosmosMsg>> {
    let mut msgs: Vec<CosmosMsg> = vec![to_asset.into_msg(None, &deps.querier, recipient)?];

    match from_asset.info {
        AssetInfo::NativeToken { denom: _ } => {
            if is_mint_burn {
                return Err(StdError::generic_err(
                    "With mint_burn mechanism, to_token must be cw20 token",
                ));
            }
        }
        AssetInfo::Token { contract_addr } => {
            if is_mint_burn {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::Burn {
                        amount: from_asset.amount,
                    })?,
                    funds: vec![],
                }))
            }
        }
    }

    Ok(msgs)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::ConvertInfo { asset_info } => {
            to_json_binary(&query_convert_info(deps, asset_info)?)
        }
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let state = read_config(deps.storage)?;
    let resp = ConfigResponse {
        owner: deps.api.addr_humanize(&state.owner)?,
    };

    Ok(resp)
}

pub fn query_convert_info(deps: Deps, asset_info: AssetInfo) -> StdResult<ConvertInfoResponse> {
    let asset_key = asset_info.to_vec(deps.api)?;
    let token_ratio = read_token_ratio(deps.storage, &asset_key)?;
    Ok(ConvertInfoResponse { token_ratio })
}

pub fn withdraw_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_infos: Vec<AssetInfo>,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let owner = deps.api.addr_humanize(&config.owner)?;
    if owner != info.sender {
        return Err(StdError::generic_err("unauthorized"));
    }
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut attributes: Vec<Attribute> = vec![("action", "withdraw_tokens").into()];

    for asset in asset_infos {
        let balance = asset.query_pool(&deps.querier, env.contract.address.clone())?;
        let message = Asset {
            info: asset,
            amount: balance,
        }
        .into_msg(None, &deps.querier, owner.clone())?;
        messages.push(message);
        attributes.push(("amount", balance.to_string()).into())
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
