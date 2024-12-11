use std::collections::HashMap;

use cosmwasm_std::{
    coin, to_json_binary, Addr, Api, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
};
use oraiswap::error::ContractError;

use oraiswap_v3::token_amount::TokenAmount;
use oraiswap_v3::{sqrt_price::SqrtPrice, PoolKey};
use oraiswap_v3::{MAX_TICK, MIN_TICK};

use crate::state::{Config, CONFIG};

use cw20::Cw20ExecuteMsg;
use oraiswap::asset::{Asset, AssetInfo, PairInfo};
use oraiswap::mixed_router::{Affiliate, ExecuteMsg, SwapOperation};
use oraiswap::oracle::OracleContract;
use oraiswap::pair::{ExecuteMsg as PairExecuteMsg, PairExecuteMsgCw20, QueryMsg as PairQueryMsg};
use oraiswap::querier::{query_pair_config, query_pair_info, query_token_balance};
use oraiswap_v3::msg::ExecuteMsg as OraiswapV3ExecuteMsg;

/// Execute swap operation
/// swap all offer asset to ask asset
pub fn execute_swap_operation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operation: SwapOperation,
    to: Option<Addr>,
    sender: Addr,
) -> Result<Response, ContractError> {
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let config: Config = CONFIG.load(deps.storage)?;
    let oraiswap_v3 = deps.api.addr_humanize(&config.oraiswap_v3)?;
    let factory_addr = deps.api.addr_humanize(&config.factory_addr)?;
    let factory_addr_v2 = deps.api.addr_humanize(&config.factory_addr_v2)?;
    let pair_config = query_pair_config(&deps.querier, factory_addr.clone())
        .or_else(|_| query_pair_config(&deps.querier, factory_addr_v2.clone()))?;
    let oracle_contract = OracleContract(pair_config.oracle_addr.clone());

    let messages: Vec<CosmosMsg> = match operation {
        SwapOperation::OraiSwap {
            offer_asset_info,
            ask_asset_info,
        } => {
            let pair_info: PairInfo = query_pair_info(
                &deps.querier,
                factory_addr,
                &[offer_asset_info.clone(), ask_asset_info.clone()],
            )
            .or_else(|_| -> StdResult<PairInfo> {
                query_pair_info(
                    &deps.querier,
                    factory_addr_v2.clone(),
                    &[offer_asset_info.clone(), ask_asset_info.clone()],
                )
            })?;

            // If there is an error, the default is for the pool to be open to everyone
            let is_whitelisted = deps
                .querier
                .query_wasm_smart(
                    pair_info.contract_addr.to_string(),
                    &PairQueryMsg::TraderIsWhitelisted { trader: sender },
                )
                .unwrap_or(true);

            if !is_whitelisted {
                return Err(ContractError::PoolWhitelisted {});
            }

            let amount = match offer_asset_info.clone() {
                AssetInfo::NativeToken { denom } => {
                    deps.querier
                        .query_balance(env.contract.address, denom)?
                        .amount
                }
                AssetInfo::Token { contract_addr } => {
                    query_token_balance(&deps.querier, contract_addr, env.contract.address)?
                }
            };
            let offer_asset: Asset = Asset {
                info: offer_asset_info,
                amount,
            };

            // swap token in smart contract
            vec![asset_into_swap_msg(
                deps.as_ref(),
                &oracle_contract,
                pair_info.contract_addr,
                offer_asset,
                None,
                to,
            )?]
        }
        SwapOperation::SwapV3 { pool_key, x_to_y } => {
            // only support swap by amount in
            let (offer_asset_info, _) = get_swap_v3_asset_info(deps.api, &pool_key, &x_to_y);

            let mut msgs: Vec<CosmosMsg> = vec![];

            let sqrt_price_limit = if x_to_y {
                SqrtPrice::from_tick(MIN_TICK).unwrap()
            } else {
                SqrtPrice::from_tick(MAX_TICK).unwrap()
            };

            match offer_asset_info.clone() {
                AssetInfo::NativeToken { denom } => {
                    let balance = deps
                        .querier
                        .query_balance(env.contract.address, denom.clone())?
                        .amount;
                    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: oraiswap_v3.to_string(),
                        msg: to_json_binary(&OraiswapV3ExecuteMsg::Swap {
                            pool_key,
                            x_to_y,
                            amount: TokenAmount(balance.into()),
                            by_amount_in: true,
                            sqrt_price_limit,
                        })
                        .unwrap(),
                        funds: vec![coin(balance.into(), denom)],
                    }))
                }
                AssetInfo::Token { contract_addr } => {
                    let balance = query_token_balance(
                        &deps.querier,
                        contract_addr.clone(),
                        env.contract.address,
                    )?;
                    // approve first
                    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: contract_addr.to_string(),
                        msg: to_json_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                            spender: oraiswap_v3.to_string(),
                            amount: balance.clone(),
                            expires: None,
                        })?,
                        funds: vec![],
                    }));
                    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: oraiswap_v3.to_string(),
                        msg: to_json_binary(&OraiswapV3ExecuteMsg::Swap {
                            pool_key,
                            x_to_y,
                            amount: TokenAmount(balance.into()),
                            by_amount_in: true,
                            sqrt_price_limit,
                        })
                        .unwrap(),
                        funds: vec![],
                    }))
                }
            };

            msgs
        }
    };

    Ok(Response::new().add_messages(messages))
}

pub fn execute_swap_operations(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    operations: Vec<SwapOperation>,
    minimum_receive: Option<Uint128>,
    to: Option<Addr>,
    affiliates: Vec<Affiliate>,
) -> Result<Response, ContractError> {
    let operations_len = operations.len();
    if operations_len == 0 {
        return Err(ContractError::NoSwapOperation {});
    }

    // Assert the operations are properly set
    assert_operations(deps.api, &operations)?;

    let to = to.unwrap_or(sender.clone());

    let target_asset_info = operations.last().unwrap().get_target_asset_info(deps.api);

    let mut operation_index = 0;
    let mut messages: Vec<CosmosMsg> = operations
        .into_iter()
        .map(|op| {
            operation_index += 1;
            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                funds: vec![],
                msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperation {
                    operation: op,
                    to: None,
                    sender: sender.clone(),
                })?,
            }))
        })
        .collect::<StdResult<Vec<CosmosMsg>>>()?;

    // Execute minimum amount assertion
    let minimum_receive = minimum_receive.unwrap_or_default();

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: to_json_binary(&ExecuteMsg::AssertMinimumReceiveAndTransfer {
            asset_info: target_asset_info,
            minimum_receive,
            receiver: to,
            affiliates,
        })?,
    }));

    Ok(Response::new().add_messages(messages))
}

fn asset_into_swap_msg(
    deps: Deps,
    oracle_contract: &OracleContract,
    pair_contract: Addr,
    offer_asset: Asset,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> StdResult<CosmosMsg> {
    match offer_asset.info.clone() {
        AssetInfo::NativeToken { denom } => {
            let return_asset = Asset {
                info: AssetInfo::NativeToken {
                    denom: denom.clone(),
                },
                amount: offer_asset.amount,
            };

            // deduct tax first
            let amount = offer_asset
                .amount
                .checked_sub(return_asset.compute_tax(oracle_contract, &deps.querier)?)?;

            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pair_contract.to_string(),
                funds: vec![Coin { denom, amount }],
                msg: to_json_binary(&PairExecuteMsg::Swap {
                    offer_asset: Asset {
                        amount,
                        ..offer_asset
                    },
                    belief_price: None,
                    max_spread,
                    to,
                })?,
            }))
        }
        AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            funds: vec![],
            msg: to_json_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract.to_string(),
                amount: offer_asset.amount,
                msg: to_json_binary(&PairExecuteMsgCw20::Swap {
                    belief_price: None,
                    max_spread,
                    to,
                })?,
            })?,
        })),
    }
}

pub fn assert_operations(api: &dyn Api, operations: &[SwapOperation]) -> StdResult<()> {
    let mut ask_asset_map: HashMap<String, bool> = HashMap::new();
    for operation in operations.iter() {
        let (offer_asset, ask_asset) = match operation {
            SwapOperation::OraiSwap {
                offer_asset_info,
                ask_asset_info,
            } => (offer_asset_info.clone(), ask_asset_info.clone()),
            SwapOperation::SwapV3 { pool_key, x_to_y } => {
                get_swap_v3_asset_info(api, pool_key, x_to_y)
            }
        };

        ask_asset_map.remove(&offer_asset.to_string());
        ask_asset_map.insert(ask_asset.to_string(), true);
    }

    if ask_asset_map.keys().len() != 1 {
        return Err(StdError::generic_err(
            "invalid operations; multiple output token",
        ));
    }

    Ok(())
}

pub fn get_swap_v3_asset_info(
    api: &dyn Api,
    pool_key: &PoolKey,
    x_to_y: &bool,
) -> (AssetInfo, AssetInfo) {
    if *x_to_y {
        (
            AssetInfo::from_denom(api, &pool_key.token_x),
            AssetInfo::from_denom(api, &pool_key.token_y),
        )
    } else {
        (
            AssetInfo::from_denom(api, &pool_key.token_y),
            AssetInfo::from_denom(api, &pool_key.token_x),
        )
    }
}
