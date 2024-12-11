
use cosmwasm_std::{Addr, Coin, Decimal, Uint128};
use oraiswap::asset::{Asset, AssetInfo, ORAI_DENOM};
use oraiswap::create_entry_points_testing;
use oraiswap::router::SwapOperation;
use oraiswap::smartrouter::{
    ExecuteMsg, GetSmartRouteResponse, InstantiateMsg, QueryMsg, SmartRouteMode,
};

use oraiswap::testing::{MockApp, ATOM_DENOM};

#[test]
fn get_smart_router_test() {
    // fixture
    let owner = "addr0000";
    let oraix_token = "ORAIX";
    let usdc_token = "USDC";
    let orai_info = AssetInfo::NativeToken {
        denom: ORAI_DENOM.to_string(),
    };
    let mut app = MockApp::new(&[(
        owner,
        &[Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(10000u128),
        }],
    )]);

    app.set_oracle_contract(Box::new(create_entry_points_testing!(oraiswap_oracle)));

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    app.set_factory_and_pair_contract(
        Box::new(
            create_entry_points_testing!(oraiswap_factory)
                .with_reply_empty(oraiswap_factory::contract::reply),
        ),
        Box::new(
            create_entry_points_testing!(oraiswap_pair)
                .with_reply_empty(oraiswap_pair::contract::reply),
        ),
    );

    app.set_router_contract(
        Box::new(create_entry_points_testing!(oraiswap_router)),
        app.factory_addr.clone(),
    );

    // set tax rate as 0.3%
    app.set_tax(
        Decimal::permille(3),
        &[
            (&ORAI_DENOM.to_string(), 10000000u128),
            (&ATOM_DENOM.to_string(), 10000000u128),
        ],
    );

    let oraix_addr = app.create_token(oraix_token);
    let usdc_addr = app.create_token(usdc_token);
    let oraix_info = AssetInfo::Token {
        contract_addr: oraix_addr.clone(),
    };
    let usdc_info = AssetInfo::Token {
        contract_addr: usdc_addr.clone(),
    };
    app.set_token_balances(&[(oraix_token, &[(owner, 1000000000u128)])])
        .unwrap();

    app.set_token_balances(&[(usdc_token, &[(owner, 1000000000u128)])])
        .unwrap();

    let oraix_usdc_pair = [oraix_info.clone(), usdc_info.clone()];
    app.create_pair(oraix_usdc_pair.clone()).unwrap();

    let oraix_orai_pair = [oraix_info.clone(), orai_info.clone()];
    app.create_pair(oraix_orai_pair.clone()).unwrap();
    let orai_usdc_pair = [orai_info.clone(), usdc_info.clone()];
    app.create_pair(orai_usdc_pair.clone()).unwrap();

    // provide liquidity
    // successfully provide liquidity for the exist pool
    let oraix_usdc_pair = app.query_pair(oraix_usdc_pair.clone()).unwrap();
    let oraix_orai_pair = app.query_pair(oraix_orai_pair.clone()).unwrap();
    let orai_usdc_pair = app.query_pair(orai_usdc_pair.clone()).unwrap();
    // approve pairs to spend owner's tokens before providing lp
    app.approve_token(
        oraix_token,
        owner,
        oraix_usdc_pair.contract_addr.as_str(),
        u128::MAX,
    )
    .unwrap();
    app.approve_token(
        oraix_token,
        owner,
        oraix_orai_pair.contract_addr.as_str(),
        u128::MAX,
    )
    .unwrap();
    app.approve_token(
        usdc_token,
        owner,
        oraix_usdc_pair.contract_addr.as_str(),
        u128::MAX,
    )
    .unwrap();
    app.approve_token(
        usdc_token,
        owner,
        orai_usdc_pair.contract_addr.as_str(),
        u128::MAX,
    )
    .unwrap();
    // provide lp for pair oraix/usdc
    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: oraix_info.clone(),
                amount: Uint128::from(100u128),
            },
            Asset {
                info: usdc_info.clone(),
                amount: Uint128::from(100u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    let _res = app
        .execute(
            Addr::unchecked(owner),
            oraix_usdc_pair.contract_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();

    // provide lp for oraix/orai
    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: oraix_info.clone(),
                amount: Uint128::from(5000u128),
            },
            Asset {
                info: orai_info.clone(),
                amount: Uint128::from(1000u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    let _res = app
        .execute(
            Addr::unchecked(owner),
            oraix_orai_pair.contract_addr.clone(),
            &msg,
            &vec![Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            }],
        )
        .unwrap();

    // provide lp for orai/usdc
    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: orai_info.clone(),
                amount: Uint128::from(1000u128),
            },
            Asset {
                info: usdc_info.clone(),
                amount: Uint128::from(5000u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    let _res = app
        .execute(
            Addr::unchecked(owner),
            orai_usdc_pair.contract_addr.clone(),
            &msg,
            &vec![Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            }],
        )
        .unwrap();

    let msg = InstantiateMsg {
        owner: owner.to_string(),
        router_addr: app.router_addr.to_string(),
    };

    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));

    // we can just call .unwrap() to assert this was a success
    let smart_router_addr = app
        .instantiate(
            code_id,
            Addr::unchecked("addr0000"),
            &msg,
            &[],
            "smart-router",
        )
        .unwrap();

    app.execute(
        Addr::unchecked(owner),
        smart_router_addr.clone(),
        &ExecuteMsg::SetRoute {
            input_info: orai_info.clone(),
            output_info: oraix_info.clone(),
            pool_route: vec![SwapOperation::OraiSwap {
                offer_asset_info: orai_info.clone(),
                ask_asset_info: oraix_info.clone(),
            }],
        },
        &[],
    )
    .unwrap();
    app.execute(
        Addr::unchecked(owner),
        smart_router_addr.clone(),
        &ExecuteMsg::SetRoute {
            input_info: orai_info.clone(),
            output_info: usdc_info.clone(),
            pool_route: vec![SwapOperation::OraiSwap {
                offer_asset_info: orai_info.clone(),
                ask_asset_info: usdc_info.clone(),
            }],
        },
        &[],
    )
    .unwrap();
    app.execute(
        Addr::unchecked(owner),
        smart_router_addr.clone(),
        &ExecuteMsg::SetRoute {
            input_info: orai_info.clone(),
            output_info: usdc_info.clone(),
            pool_route: vec![
                SwapOperation::OraiSwap {
                    offer_asset_info: orai_info.clone(),
                    ask_asset_info: oraix_info.clone(),
                },
                SwapOperation::OraiSwap {
                    offer_asset_info: oraix_info.clone(),
                    ask_asset_info: usdc_info.clone(),
                },
            ],
        },
        &[],
    )
    .unwrap();

    // this route is faulty for testing. the pair doenst exist
    app.execute(
        Addr::unchecked(owner),
        smart_router_addr.clone(),
        &ExecuteMsg::SetRoute {
            input_info: orai_info.clone(),
            output_info: AssetInfo::NativeToken {
                denom: "random-denom".to_string(),
            },
            pool_route: vec![SwapOperation::OraiSwap {
                offer_asset_info: orai_info.clone(),
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "random-denom".to_string(),
                },
            }],
        },
        &[],
    )
    .unwrap();

    // case 1: get smart route with simulate problem -> return error
    let err = app
        .query::<GetSmartRouteResponse, _>(
            smart_router_addr.clone(),
            &QueryMsg::GetSmartRoute {
                input_info: orai_info.clone(),
                output_info: AssetInfo::NativeToken {
                    denom: "random-denom".to_string(),
                },
                offer_amount: Uint128::from(100u128),
                route_mode: Some(SmartRouteMode::MaxMinimumReceive),
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains(
        "Querier contract error: Generic error: Minimum receive of simulate smart route is 0."
    ));
    assert!(err
        .to_string()
        .contains("Querier contract error: type: oraiswap::asset::PairInfoRaw"));
    let max_minimum_receive = app
        .query::<GetSmartRouteResponse, _>(
            smart_router_addr.clone(),
            &QueryMsg::GetSmartRoute {
                input_info: orai_info.clone(),
                output_info: usdc_info.clone(),
                offer_amount: Uint128::from(100u128),
                route_mode: Some(SmartRouteMode::MaxMinimumReceive),
            },
        )
        .unwrap();

    // assertion
    // we expect the actual min receive of mode NearestMinimumReceive to be nearer to the expected_min_receive than the mode FurthestMinimumReceive
    assert_eq!(max_minimum_receive.swap_ops.len(), 1);
}
