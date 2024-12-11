use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_json, Addr, Coin, DepsMut, StdError};
use oraiswap::asset::AssetInfo;
use oraiswap::router::SwapOperation;

use crate::{contract, ContractError};
use oraiswap::smartrouter::{
    ExecuteMsg, GetConfigResponse, GetRouteResponse, GetRoutesResponse, InstantiateMsg, QueryMsg,
};

static CREATOR_ADDRESS: &str = "creator";

// test helper
#[allow(unused_assignments)]
fn initialize_contract(deps: DepsMut) -> Addr {
    let msg = InstantiateMsg {
        owner: String::from(CREATOR_ADDRESS),
        router_addr: "router_addr".to_string(),
    };
    let info = mock_info(CREATOR_ADDRESS, &[]);

    // instantiate with enough funds provided should succeed
    contract::instantiate(deps, mock_env(), info.clone(), msg).unwrap();

    info.sender
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies();

    let owner = initialize_contract(deps.as_mut());

    // it worked, let's query the state
    let res: GetConfigResponse =
        from_json(&contract::query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap())
            .unwrap();
    assert_eq!(owner, res.owner);
}

#[test]
fn proper_update_state() {
    let mut deps = mock_dependencies();

    let owner = initialize_contract(deps.as_mut());

    // it worked, let's query the state
    let res: GetConfigResponse =
        from_json(&contract::query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap())
            .unwrap();
    assert_eq!(owner, res.owner);

    let good_addr = "new_owner".to_string();

    let other_info = mock_info("other_sender", &vec![] as &Vec<Coin>);
    let owner_info = mock_info(owner.as_str(), &vec![] as &Vec<Coin>);

    // valid addr, bad sender
    let msg = ExecuteMsg::UpdateConfig {
        new_owner: Some(good_addr.clone()),
        new_router: None,
    };
    contract::execute(deps.as_mut(), mock_env(), other_info, msg).unwrap_err();

    // and transfer ownership
    let msg = ExecuteMsg::UpdateConfig {
        new_owner: Some(good_addr.clone()),
        new_router: Some("new_router".to_string()),
    };
    contract::execute(deps.as_mut(), mock_env(), owner_info, msg).unwrap();

    let res: GetConfigResponse =
        from_json(&contract::query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap())
            .unwrap();
    assert_eq!(good_addr, res.owner);
    assert_eq!(res.router, "new_router");
}

#[test]
fn test_set_and_get_route() {
    let mut deps = mock_dependencies();
    let owner = initialize_contract(deps.as_mut());
    let orai = AssetInfo::NativeToken {
        denom: "orai".to_string(),
    };
    let usdc = AssetInfo::Token {
        contract_addr: Addr::unchecked("usdc"),
    };
    let oraix = AssetInfo::Token {
        contract_addr: Addr::unchecked("oraix"),
    };
    let orai_usdc_simple_ops = vec![SwapOperation::OraiSwap {
        offer_asset_info: orai.clone(),
        ask_asset_info: usdc.clone(),
    }];
    let orai_usdc_oraix_ops = vec![
        SwapOperation::OraiSwap {
            offer_asset_info: orai.clone(),
            ask_asset_info: oraix.clone(),
        },
        SwapOperation::OraiSwap {
            offer_asset_info: oraix.clone(),
            ask_asset_info: usdc.clone(),
        },
    ];
    let set_route_msg = ExecuteMsg::SetRoute {
        input_info: orai.clone(),
        output_info: usdc.clone(),
        pool_route: orai_usdc_simple_ops.clone(),
    };

    // case 1: unauthorized
    let err = contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info("any", &vec![]),
        set_route_msg.clone(),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), ContractError::Unauthorized {}.to_string());

    // case 2: valid route orai->usdc
    contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info(owner.as_str(), &vec![]),
        set_route_msg,
    )
    .unwrap();

    // case 3: valid route orai->oraix->usdc
    contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info(owner.as_str(), &vec![]),
        ExecuteMsg::SetRoute {
            input_info: orai.clone(),
            output_info: usdc.clone(),
            pool_route: orai_usdc_oraix_ops.clone(),
        },
    )
    .unwrap();

    // try querying all the routes. Should return 2 routes
    let routes: GetRoutesResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoutes {
                input_info: orai.clone(),
                output_info: usdc.clone(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(routes.pool_routes.len(), 2);

    // try querying the first route
    let route: GetRouteResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoute {
                input_info: orai.clone(),
                output_info: usdc.clone(),
                route_index: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(route.pool_route, orai_usdc_simple_ops.clone());

    // try querying the 2nd route
    let route: GetRouteResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoute {
                input_info: orai.clone(),
                output_info: usdc.clone(),
                route_index: 1,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(route.pool_route, orai_usdc_oraix_ops.clone());

    // try querying the 3rd route. Should return error
    let err = &contract::query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::GetRoute {
            input_info: orai.clone(),
            output_info: usdc.clone(),
            route_index: 2,
        },
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        StdError::generic_err("Could not find route given the route index").to_string()
    );
}

#[test]
fn test_set_reversed_route() {
    let mut deps = mock_dependencies();
    let owner = initialize_contract(deps.as_mut());
    let orai = AssetInfo::NativeToken {
        denom: "orai".to_string(),
    };
    let usdc = AssetInfo::Token {
        contract_addr: Addr::unchecked("usdc"),
    };
    let oraix = AssetInfo::Token {
        contract_addr: Addr::unchecked("oraix"),
    };
    let orai_usdc_simple_ops = vec![SwapOperation::OraiSwap {
        offer_asset_info: orai.clone(),
        ask_asset_info: usdc.clone(),
    }];
    let orai_usdc_oraix_ops = vec![
        SwapOperation::OraiSwap {
            offer_asset_info: orai.clone(),
            ask_asset_info: oraix.clone(),
        },
        SwapOperation::OraiSwap {
            offer_asset_info: oraix.clone(),
            ask_asset_info: usdc.clone(),
        },
    ];

    // case 1: valid route orai->usdc, should also add usdc->orai
    contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info(owner.as_str(), &vec![]),
        ExecuteMsg::SetRoute {
            input_info: orai.clone(),
            output_info: usdc.clone(),
            pool_route: orai_usdc_simple_ops.clone(),
        },
    )
    .unwrap();

    // case 3: valid route orai->oraix->usdc, should also add usdc->oraix->orai
    contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info(owner.as_str(), &vec![]),
        ExecuteMsg::SetRoute {
            input_info: orai.clone(),
            output_info: usdc.clone(),
            pool_route: orai_usdc_oraix_ops.clone(),
        },
    )
    .unwrap();

    // try querying all the routes of usdc->orai. Should return 2 routes
    let routes: GetRoutesResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoutes {
                input_info: usdc.clone(),
                output_info: orai.clone(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(routes.pool_routes.len(), 2);

    // try querying the first route
    let route: GetRouteResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoute {
                input_info: usdc.clone(),
                output_info: orai.clone(),
                route_index: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        route.pool_route,
        orai_usdc_simple_ops
            .iter()
            .map(|op| match op {
                SwapOperation::OraiSwap {
                    offer_asset_info,
                    ask_asset_info,
                } => SwapOperation::OraiSwap {
                    offer_asset_info: ask_asset_info.to_owned(),
                    ask_asset_info: offer_asset_info.to_owned()
                },
            })
            .collect::<Vec<SwapOperation>>()
    );

    // try querying the 2nd route
    let route: GetRouteResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoute {
                input_info: usdc.clone(),
                output_info: orai.clone(),
                route_index: 1,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let mut reversed_orai_usdc_oraix_ops = orai_usdc_oraix_ops.to_owned();
    reversed_orai_usdc_oraix_ops.reverse();
    assert_eq!(
        route.pool_route,
        reversed_orai_usdc_oraix_ops
            .iter()
            .map(|op| match op {
                SwapOperation::OraiSwap {
                    offer_asset_info,
                    ask_asset_info,
                } => SwapOperation::OraiSwap {
                    offer_asset_info: ask_asset_info.to_owned(),
                    ask_asset_info: offer_asset_info.to_owned()
                },
            })
            .collect::<Vec<SwapOperation>>()
    );

    // try querying the 3rd route. Should return error
    let err = &contract::query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::GetRoute {
            input_info: orai.clone(),
            output_info: usdc.clone(),
            route_index: 2,
        },
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        StdError::generic_err("Could not find route given the route index").to_string()
    );
}

#[test]
fn test_delete_route() {
    let mut deps = mock_dependencies();
    let owner = initialize_contract(deps.as_mut());
    let orai = AssetInfo::NativeToken {
        denom: "orai".to_string(),
    };
    let usdc = AssetInfo::Token {
        contract_addr: Addr::unchecked("usdc"),
    };
    let oraix = AssetInfo::Token {
        contract_addr: Addr::unchecked("oraix"),
    };
    let orai_usdc_simple_ops = vec![SwapOperation::OraiSwap {
        offer_asset_info: orai.clone(),
        ask_asset_info: usdc.clone(),
    }];
    let orai_usdc_oraix_ops = vec![
        SwapOperation::OraiSwap {
            offer_asset_info: orai.clone(),
            ask_asset_info: oraix.clone(),
        },
        SwapOperation::OraiSwap {
            offer_asset_info: oraix.clone(),
            ask_asset_info: usdc.clone(),
        },
    ];

    contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info(owner.as_str(), &vec![]),
        ExecuteMsg::SetRoute {
            input_info: orai.clone(),
            output_info: usdc.clone(),
            pool_route: orai_usdc_simple_ops.clone(),
        },
    )
    .unwrap();

    contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info(owner.as_str(), &vec![]),
        ExecuteMsg::SetRoute {
            input_info: orai.clone(),
            output_info: usdc.clone(),
            pool_route: orai_usdc_oraix_ops.clone(),
        },
    )
    .unwrap();

    // try querying all the routes of usdc->orai. Should return 2 routes
    let routes: GetRoutesResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoutes {
                input_info: usdc.clone(),
                output_info: orai.clone(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(routes.pool_routes.len(), 2);

    let delete_route_msg = ExecuteMsg::DeleteRoute {
        input_info: orai.clone(),
        output_info: usdc.clone(),
        route_index: 0,
    };
    // case 1: unauthorized. Cannot delete
    let err = contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info("any", &vec![]),
        delete_route_msg.clone(),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), ContractError::Unauthorized {}.to_string());

    // case 2: delete successfully
    // case 1: unauthorized. Cannot delete
    contract::execute(
        deps.as_mut(),
        mock_env(),
        mock_info(owner.as_str(), &vec![]),
        delete_route_msg,
    )
    .unwrap();

    // try querying, now we only have one route left, which is the orai-oraix-usdc route
    let routes: GetRoutesResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoutes {
                input_info: orai.clone(),
                output_info: usdc.clone(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(routes.pool_routes.len(), 1);

    let route: GetRouteResponse = from_json(
        &contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetRoute {
                input_info: orai.clone(),
                output_info: usdc.clone(),
                route_index: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(route.pool_route, orai_usdc_oraix_ops.clone());
}
