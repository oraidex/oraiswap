use cosmwasm_std::{Addr, Coin, Decimal, Uint128};
use oraiswap::asset::{Asset, AssetInfo, ORAI_DENOM};
use oraiswap::create_entry_points_testing;
use oraiswap::router::{
    ExecuteMsg, InstantiateMsg, QueryMsg, SimulateSwapOperationsResponse, SwapOperation,
};

use oraiswap::testing::{MockApp, ATOM_DENOM};

#[test]
fn simulate_swap_operations_test() {
    let mut app = MockApp::new(&[(
        "addr0000",
        &[
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            },
            Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            },
        ],
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

    // set tax rate as 0.3%
    app.set_tax(
        Decimal::permille(3),
        &[
            (&ORAI_DENOM.to_string(), 10000000u128),
            (&ATOM_DENOM.to_string(), 10000000u128),
        ],
    );

    let asset_infos = [
        AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        AssetInfo::NativeToken {
            denom: ATOM_DENOM.to_string(),
        },
    ];

    // create pair
    let pair_addr = app.create_pair(asset_infos.clone()).unwrap();

    // provide liquidity
    // successfully provide liquidity for the exist pool
    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(100u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            pair_addr.clone(),
            &msg,
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(100u128),
                },
                Coin {
                    denom: ATOM_DENOM.to_string(),
                    amount: Uint128::from(100u128),
                },
            ],
        )
        .unwrap();

    let msg = InstantiateMsg {
        factory_addr: app.factory_addr.clone(),
        factory_addr_v2: Addr::unchecked("addr0000_v2"),
    };

    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));

    // we can just call .unwrap() to assert this was a success
    let router_addr = app
        .instantiate(code_id, Addr::unchecked("addr0000"), &msg, &[], "router")
        .unwrap();

    let msg = QueryMsg::SimulateSwapOperations {
        offer_amount: Uint128::from(100u128),
        operations: vec![SwapOperation::OraiSwap {
            offer_asset_info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
            ask_asset_info: AssetInfo::NativeToken {
                denom: ATOM_DENOM.to_string(),
            },
        }],
    };

    let res: SimulateSwapOperationsResponse = app.query(router_addr, &msg).unwrap();
    println!("{:?}", res);
}

#[test]
fn execute_swap_operations() {
    let mut app = MockApp::new(&[(
        "addr0000",
        &[
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            },
            Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            },
        ],
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
    // set tax rate as 0.3%
    app.set_tax(
        Decimal::permille(3),
        &[
            (&ORAI_DENOM.to_string(), 10000000u128),
            (&ATOM_DENOM.to_string(), 10000000u128),
        ],
    );

    let asset_addr = app.create_token("asset");

    app.set_token_balances(&[("asset", &[("addr0000", 1000000u128)])])
        .unwrap();

    let asset_infos1 = [
        AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        AssetInfo::Token {
            contract_addr: asset_addr.clone(),
        },
    ];

    let asset_infos2 = [
        AssetInfo::NativeToken {
            denom: ATOM_DENOM.to_string(),
        },
        AssetInfo::Token {
            contract_addr: asset_addr.clone(),
        },
    ];

    // create pair
    let pair_addr1 = app.create_pair(asset_infos1.clone()).unwrap();
    let pair_addr2 = app.create_pair(asset_infos2.clone()).unwrap();

    // provide liquidity
    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: asset_addr.clone(),
                },
                amount: Uint128::from(100u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    // set allowance
    app.execute(
        Addr::unchecked("addr0000"),
        asset_addr.clone(),
        &cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: pair_addr1.to_string(),
            amount: Uint128::from(100u128),
            expires: None,
        },
        &[],
    )
    .unwrap();

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            pair_addr1.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(100u128),
            }],
        )
        .unwrap();

    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: asset_addr.clone(),
                },
                amount: Uint128::from(100u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    // set allowance
    app.execute(
        Addr::unchecked("addr0000"),
        asset_addr.clone(),
        &cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: pair_addr2.to_string(),
            amount: Uint128::from(100u128),
            expires: None,
        },
        &[],
    )
    .unwrap();

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            pair_addr2.clone(),
            &msg,
            &[Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(100u128),
            }],
        )
        .unwrap();

    let msg = InstantiateMsg {
        factory_addr: app.factory_addr.clone(),
        factory_addr_v2: Addr::unchecked("addr0000_v2"),
    };

    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));

    // we can just call .unwrap() to assert this was a success
    let router_addr = app
        .instantiate(code_id, Addr::unchecked("addr0000"), &msg, &[], "router")
        .unwrap();

    let msg = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![],
        minimum_receive: None,
        to: None,
    };

    let error = app
        .execute(Addr::unchecked("addr0000"), router_addr.clone(), &msg, &[])
        .unwrap_err();
    assert!(error
        .root_cause()
        .to_string()
        .contains("must provide operations"));

    let msg = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![
            SwapOperation::OraiSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: asset_addr.clone(),
                },
            },
            SwapOperation::OraiSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: asset_addr.clone(),
                },
            },
        ],
        minimum_receive: None,
        to: None,
    };

    let res = app
        .execute(
            Addr::unchecked("addr0000"),
            router_addr.clone(),
            &msg,
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(100u128),
                },
                Coin {
                    denom: ATOM_DENOM.to_string(),
                    amount: Uint128::from(100u128),
                },
            ],
        )
        .unwrap();

    println!("{:?}", res.events);

    // test swap on whitelisted pool

    // enable  whitelisted pool
    let _res = app
        .execute(
            Addr::unchecked("admin"),
            pair_addr1.clone(),
            &oraiswap::pair::ExecuteMsg::EnableWhitelist { status: true },
            &[],
        )
        .unwrap();
    let _res = app
        .execute(
            Addr::unchecked("admin"),
            pair_addr2.clone(),
            &oraiswap::pair::ExecuteMsg::EnableWhitelist { status: true },
            &[],
        )
        .unwrap();

    // whitelist swap router
    app.execute(
        Addr::unchecked("admin"),
        pair_addr1.clone(),
        &oraiswap::pair::ExecuteMsg::RegisterTrader {
            traders: vec![router_addr.clone()],
        },
        &[],
    )
    .unwrap();
    app.execute(
        Addr::unchecked("admin"),
        pair_addr2.clone(),
        &oraiswap::pair::ExecuteMsg::RegisterTrader {
            traders: vec![router_addr.clone()],
        },
        &[],
    )
    .unwrap();

    // swap will be failed

    let error = app
        .execute(
            Addr::unchecked("addr0000"),
            router_addr.clone(),
            &msg,
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(100u128),
                },
                Coin {
                    denom: ATOM_DENOM.to_string(),
                    amount: Uint128::from(100u128),
                },
            ],
        )
        .unwrap_err();

    assert!(error
        .root_cause()
        .to_string()
        .contains("This pool is not open to everyone, only whitelisted traders can swap"));

    // whitelist trader
    app.execute(
        Addr::unchecked("admin"),
        pair_addr1.clone(),
        &oraiswap::pair::ExecuteMsg::RegisterTrader {
            traders: vec![Addr::unchecked("addr0000")],
        },
        &[],
    )
    .unwrap();
    app.execute(
        Addr::unchecked("admin"),
        pair_addr2.clone(),
        &oraiswap::pair::ExecuteMsg::RegisterTrader {
            traders: vec![Addr::unchecked("addr0000")],
        },
        &[],
    )
    .unwrap();

    // swap successfully
    app.execute(
        Addr::unchecked("addr0000"),
        router_addr.clone(),
        &msg,
        &[
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(100u128),
            },
            Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(100u128),
            },
        ],
    )
    .unwrap();
}
