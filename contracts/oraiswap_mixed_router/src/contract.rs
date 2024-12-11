#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    attr, from_json, to_json_binary, Addr, Attribute, Binary, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128,
};
use oraiswap::error::ContractError;
use oraiswap_v3::interface::QuoteResult;
use oraiswap_v3::msg::QueryMsg as SwapV3QueryMsg;
use oraiswap_v3::sqrt_price::SqrtPrice;
use oraiswap_v3::token_amount::TokenAmount;
use oraiswap_v3::{MAX_TICK, MIN_TICK};

use crate::operations::{execute_swap_operation, execute_swap_operations};
use crate::state::{Config, CONFIG};

use cw20::Cw20ReceiveMsg;
use oraiswap::asset::{Asset, AssetInfo, PairInfo};
use oraiswap::mixed_router::{
    Affiliate, ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    SimulateSwapOperationsResponse, SwapOperation,
};
use oraiswap::oracle::OracleContract;
use oraiswap::pair::{QueryMsg as PairQueryMsg, SimulationResponse};
use oraiswap::querier::{query_pair_config, query_pair_info};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    CONFIG.save(
        deps.storage,
        &Config {
            factory_addr: deps.api.addr_canonicalize(msg.factory_addr.as_str())?,
            factory_addr_v2: deps.api.addr_canonicalize(msg.factory_addr_v2.as_str())?,
            oraiswap_v3: deps.api.addr_canonicalize(msg.oraiswap_v3.as_str())?,
            owner: deps.api.addr_canonicalize(info.sender.as_str())?,
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
            affiliates,
        } => execute_swap_operations(
            deps,
            env,
            info.sender,
            operations,
            minimum_receive,
            to,
            affiliates.unwrap_or_default(),
        ),
        ExecuteMsg::ExecuteSwapOperation {
            operation,
            to,
            sender,
        } => execute_swap_operation(deps, env, info, operation, to, sender),

        ExecuteMsg::AssertMinimumReceiveAndTransfer {
            asset_info,
            minimum_receive,
            receiver,
            affiliates,
        } => assert_minium_receive_and_transfer(
            deps.as_ref(),
            env,
            asset_info,
            minimum_receive,
            receiver,
            affiliates,
        ),
        ExecuteMsg::UpdateConfig {
            factory_addr,
            factory_addr_v2,
            oraiswap_v3,
            owner,
        } => execute_update_config(
            deps,
            info,
            factory_addr,
            factory_addr_v2,
            oraiswap_v3,
            owner,
        ),
    }
}

pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    factory_addr: Option<String>,
    factory_addr_v2: Option<String>,
    oraiswap_v3: Option<String>,
    owner: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;

    // check authorized
    if config.owner.ne(&sender_addr) {
        return Err(ContractError::Unauthorized {});
    }

    // update new reward address
    if let Some(factory_addr) = factory_addr {
        config.factory_addr = deps.api.addr_canonicalize(&factory_addr)?;
    }
    if let Some(factory_addr_v2) = factory_addr_v2 {
        config.factory_addr_v2 = deps.api.addr_canonicalize(&factory_addr_v2)?;
    }
    if let Some(oraiswap_v3) = oraiswap_v3 {
        config.oraiswap_v3 = deps.api.addr_canonicalize(&oraiswap_v3)?;
    }
    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attributes(vec![("action", "execute_update_config")]))
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&cw20_msg.sender)?;

    // throw empty data as well when decoding
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
            affiliates,
        } => {
            let receiver = to.and_then(|addr| deps.api.addr_validate(addr.as_str()).ok());
            execute_swap_operations(
                deps,
                env,
                sender,
                operations,
                minimum_receive,
                receiver,
                affiliates.unwrap_or_default(),
            )
        }
    }
}

fn assert_minium_receive_and_transfer(
    deps: Deps,
    env: Env,
    asset_info: AssetInfo,
    minium_receive: Uint128,
    receiver: Addr,
    affiliates: Vec<Affiliate>,
) -> Result<Response, ContractError> {
    let mut curr_balance = asset_info.query_pool(&deps.querier, env.contract.address)?;

    if curr_balance < minium_receive {
        return Err(ContractError::SwapAssertionFailure {
            minium_receive,
            swap_amount: curr_balance,
        });
    }

    let mut asset = Asset {
        info: asset_info,
        amount: Uint128::zero(),
    };

    // Create affiliate response and total affiliate fee amount
    let mut msgs: Vec<CosmosMsg> = vec![];
    let mut attrs: Vec<Attribute> = vec![];
    let mut total_affiliate_fee_amount: Uint128 = Uint128::zero();

    // If affiliates exist, create the affiliate fee messages and attributes and
    // add them to the affiliate response, updating the total affiliate fee amount
    for affiliate in affiliates.iter() {
        // Get the affiliate fee amount by multiplying the min_asset
        // amount by the affiliate basis points fee divided by 10000
        let affiliate_fee_amount =
            curr_balance.multiply_ratio(affiliate.basis_points_fee, Uint128::new(10000));

        if affiliate_fee_amount > Uint128::zero() {
            // Add the affiliate fee amount to the total affiliate fee amount
            total_affiliate_fee_amount =
                total_affiliate_fee_amount.checked_add(affiliate_fee_amount)?;

            // Create the affiliate_fee_asset
            asset.amount = affiliate_fee_amount;

            // Create the affiliate fee message
            msgs.push(asset.into_msg(None, &deps.querier, affiliate.address.clone())?);

            // Add the affiliate attributes to the response
            attrs.push(attr("affiliate_receiver", affiliate.address.as_str()));
            attrs.push(attr("affiliate_amount", &affiliate_fee_amount.to_string()))
        }
    }

    // transfer to user
    if total_affiliate_fee_amount < curr_balance {
        curr_balance = curr_balance - total_affiliate_fee_amount;
        asset.amount = curr_balance;
        msgs.push(asset.into_msg(None, &deps.querier, receiver)?);
    }

    Ok(Response::new().add_messages(msgs).add_attributes(attrs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::SimulateSwapOperations {
            offer_amount,
            operations,
        } => to_json_binary(&simulate_swap_operations(deps, offer_amount, operations)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let state = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        factory_addr: deps.api.addr_humanize(&state.factory_addr)?,
        factory_addr_v2: deps.api.addr_humanize(&state.factory_addr_v2)?,
        oraiswap_v3: deps.api.addr_humanize(&state.oraiswap_v3)?,
    };

    Ok(resp)
}

fn simulate_swap_operations(
    deps: Deps,
    offer_amount: Uint128,
    operations: Vec<SwapOperation>,
) -> StdResult<SimulateSwapOperationsResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let oraiswap_v3 = deps.api.addr_humanize(&config.oraiswap_v3)?;
    let factory_addr = deps.api.addr_humanize(&config.factory_addr)?;
    let factory_addr_v2 = deps.api.addr_humanize(&config.factory_addr_v2)?;
    let operations_len = operations.len();
    if operations_len == 0 {
        return Err(StdError::generic_err(
            ContractError::NoSwapOperation {}.to_string(),
        ));
    }

    let mut offer_amount = offer_amount;
    for operation in operations.into_iter() {
        let pair_config = query_pair_config(&deps.querier, factory_addr.clone())
            .or_else(|_| query_pair_config(&deps.querier, factory_addr_v2.clone()))?;
        let oracle_contract = OracleContract(pair_config.oracle_addr);
        match operation {
            SwapOperation::OraiSwap {
                offer_asset_info,
                ask_asset_info,
            } => {
                let pair_info = query_pair_info(
                    &deps.querier,
                    factory_addr.clone(),
                    &[offer_asset_info.clone(), ask_asset_info.clone()],
                )
                .or_else(|_| -> StdResult<PairInfo> {
                    query_pair_info(
                        &deps.querier,
                        factory_addr_v2.clone(),
                        &[offer_asset_info.clone(), ask_asset_info.clone()],
                    )
                })?;

                let return_asset = Asset {
                    info: offer_asset_info.clone(),
                    amount: offer_amount,
                };

                // Deduct tax before querying simulation, with native token only
                offer_amount = offer_amount
                    .checked_sub(return_asset.compute_tax(&oracle_contract, &deps.querier)?)?;

                let mut res: SimulationResponse = deps.querier.query_wasm_smart(
                    pair_info.contract_addr,
                    &PairQueryMsg::Simulation {
                        offer_asset: Asset {
                            info: offer_asset_info,
                            amount: offer_amount,
                        },
                    },
                )?;

                let return_asset = Asset {
                    info: ask_asset_info,
                    amount: res.return_amount,
                };

                // Deduct tax after querying simulation, with native token only
                res.return_amount = res
                    .return_amount
                    .checked_sub(return_asset.compute_tax(&oracle_contract, &deps.querier)?)?;

                offer_amount = res.return_amount;
            }
            SwapOperation::SwapV3 { pool_key, x_to_y } => {
                // only support swap by_amount_in

                let sqrt_price_limit = if x_to_y {
                    SqrtPrice::from_tick(MIN_TICK).unwrap()
                } else {
                    SqrtPrice::from_tick(MAX_TICK).unwrap()
                };

                let res: QuoteResult = deps.querier.query_wasm_smart(
                    oraiswap_v3.to_string(),
                    &SwapV3QueryMsg::Quote {
                        pool_key,
                        x_to_y,
                        amount: TokenAmount(offer_amount.into()),
                        by_amount_in: true,
                        sqrt_price_limit,
                    },
                )?;

                offer_amount = Uint128::from(res.amount_out.0)
            }
        }
    }

    Ok(SimulateSwapOperationsResponse {
        amount: offer_amount,
    })
}
