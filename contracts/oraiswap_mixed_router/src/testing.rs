use cosmwasm_std::{to_json_binary, Addr, Coin, Decimal, Uint128};
use cw20::Cw20ExecuteMsg;
use oraiswap::asset::{Asset, AssetInfo, ORAI_DENOM};
use oraiswap::create_entry_points_testing;
use oraiswap::mixed_router::{
    Affiliate, ExecuteMsg, InstantiateMsg, QueryMsg, SimulateSwapOperationsResponse, SwapOperation,
};

use oraiswap::testing::{MockApp, APP_OWNER, ATOM_DENOM};
use oraiswap_v3::liquidity::Liquidity;
use oraiswap_v3::msg::ExecuteMsg as OraiswapV3ExecuteMsg;
use oraiswap_v3::percentage::Percentage;
use oraiswap_v3::sqrt_price::{calculate_sqrt_price, SqrtPrice};
use oraiswap_v3::{FeeTier, PoolKey, MAX_TICK, MIN_TICK};
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
        oraiswap_v3: Addr::unchecked("oraiswap_v3"),
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
        oraiswap_v3: Addr::unchecked("oraiswap_v3"),
    };

    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));

    // we can just call .unwrap() to assert this was a success
    let router_addr = app
        .instantiate(code_id, Addr::unchecked("addr0000"), &msg, &[], "router")
        .unwrap();

    let msg: ExecuteMsg = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![],
        minimum_receive: None,
        to: None,
        affiliates: None,
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
        affiliates: None,
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

fn init_v3(
    app: &mut MockApp,
    token_x: Addr,
    token_x_name: &str,
    token_y: Addr,
    token_y_name: &str,
) -> PoolKey {
    app.create_v3(Box::new(create_entry_points_testing!(oraiswap_v3)));

    let protocol_fee = Percentage(100);
    let fee_tier = FeeTier::new(protocol_fee, 10).unwrap();
    let contract_addr = app.v3_addr.clone();
    app.execute(
        Addr::unchecked(APP_OWNER),
        contract_addr.clone(),
        &OraiswapV3ExecuteMsg::AddFeeTier { fee_tier },
        &[],
    )
    .unwrap();

    let init_tick = 0;
    let init_sqrt_price = calculate_sqrt_price(init_tick).unwrap();

    app.execute(
        Addr::unchecked(APP_OWNER),
        contract_addr.clone(),
        &OraiswapV3ExecuteMsg::CreatePool {
            token_0: token_x.to_string(),
            token_1: token_y.to_string(),
            fee_tier,
            init_sqrt_price,
            init_tick,
        },
        &[],
    )
    .unwrap();

    let pool_key = PoolKey::new(token_x.to_string(), token_y.to_string(), fee_tier).unwrap();

    let lower_tick_index = -20;
    let middle_tick_index = -10;
    let upper_tick_index = 10;

    let liquidity_delta = Liquidity(1000000000000000000);
    app.approve_token(
        token_x_name,
        "addr0000",
        contract_addr.clone().as_str(),
        u128::MAX,
    )
    .unwrap();
    app.approve_token(
        token_y_name,
        "addr0000",
        contract_addr.clone().as_str(),
        u128::MAX,
    )
    .unwrap();

    app.execute(
        Addr::unchecked("addr0000"),
        contract_addr.clone(),
        &OraiswapV3ExecuteMsg::CreatePosition {
            pool_key: pool_key.clone(),
            lower_tick: lower_tick_index,
            upper_tick: upper_tick_index,
            liquidity_delta,
            slippage_limit_lower: SqrtPrice::from_tick(MIN_TICK).unwrap(),
            slippage_limit_upper: SqrtPrice::from_tick(MAX_TICK).unwrap(),
        },
        &[],
    )
    .unwrap();

    app.execute(
        Addr::unchecked("addr0000"),
        contract_addr.clone(),
        &OraiswapV3ExecuteMsg::CreatePosition {
            pool_key: pool_key.clone(),
            lower_tick: lower_tick_index - 20,
            upper_tick: middle_tick_index,
            liquidity_delta,
            slippage_limit_lower: SqrtPrice::from_tick(MIN_TICK).unwrap(),
            slippage_limit_upper: SqrtPrice::from_tick(MAX_TICK).unwrap(),
        },
        &[],
    )
    .unwrap();

    pool_key
}

#[test]
fn simulate_mixed_swap() {
    let mut app = MockApp::new(&[(
        "addr0000",
        &[
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000000000000u128),
            },
            Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(1000000000000u128),
            },
        ],
    )]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    let token_x_name = "tokenx";
    let token_y_name = "tokeny";
    let token_x = app.create_token(token_x_name);
    let token_y = app.create_token(token_y_name);

    app.set_token_balances(&[("tokenx", &[("addr0000", 1000000000000u128)])])
        .unwrap();
    app.set_token_balances(&[("tokeny", &[("addr0000", 1000000000000u128)])])
        .unwrap();

    let pool_key = init_v3(
        &mut app,
        token_x.clone(),
        token_x_name,
        token_y.clone(),
        token_y_name,
    );

    app.set_oracle_contract(Box::new(create_entry_points_testing!(oraiswap_oracle)));

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
        AssetInfo::Token {
            contract_addr: token_x.clone(),
        },
        AssetInfo::Token {
            contract_addr: token_y.clone(),
        },
    ];

    // create pair
    let pair_addr = app.create_pair(asset_infos.clone()).unwrap();

    // provide liquidity
    // successfully provide liquidity for the exist pool

    app.approve_token(
        token_x_name,
        "addr0000",
        pair_addr.clone().as_str(),
        u128::MAX,
    )
    .unwrap();
    app.approve_token(
        token_y_name,
        "addr0000",
        pair_addr.clone().as_str(),
        u128::MAX,
    )
    .unwrap();
    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_x.clone(),
                },
                amount: Uint128::from(1000000000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_y.clone(),
                },
                amount: Uint128::from(100000000u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    let _res = app
        .execute(Addr::unchecked("addr0000"), pair_addr.clone(), &msg, &[])
        .unwrap();

    let msg = InstantiateMsg {
        factory_addr: app.factory_addr.clone(),
        factory_addr_v2: Addr::unchecked("addr0000_v2"),
        oraiswap_v3: app.v3_addr.clone(),
    };

    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));

    // we can just call .unwrap() to assert this was a success
    let router_addr = app
        .instantiate(code_id, Addr::unchecked("addr0000"), &msg, &[], "router")
        .unwrap();

    let msg = QueryMsg::SimulateSwapOperations {
        offer_amount: Uint128::from(1000000u128),
        operations: vec![
            SwapOperation::OraiSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: token_x.clone(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: token_y.clone(),
                },
            },
            SwapOperation::SwapV3 {
                pool_key,
                x_to_y: false,
            },
        ],
    };

    let res: SimulateSwapOperationsResponse = app.query(router_addr, &msg).unwrap();

    assert_eq!(res.amount, Uint128::new(99500));
}

#[test]
fn execute_mixed_swap_operations() {
    let mut app = MockApp::new(&[(
        "addr0000",
        &[
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000000000000u128),
            },
            Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(1000000000000u128),
            },
        ],
    )]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    let token_x_name = "tokenx";
    let token_y_name = "tokeny";
    let token_x = app.create_token(token_x_name);
    let token_y = app.create_token(token_y_name);

    app.set_token_balances(&[("tokenx", &[("addr0000", 1000000000000u128)])])
        .unwrap();
    app.set_token_balances(&[("tokeny", &[("addr0000", 1000000000000u128)])])
        .unwrap();

    let pool_key = init_v3(
        &mut app,
        token_x.clone(),
        token_x_name,
        token_y.clone(),
        token_y_name,
    );

    app.set_oracle_contract(Box::new(create_entry_points_testing!(oraiswap_oracle)));

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
        AssetInfo::Token {
            contract_addr: token_x.clone(),
        },
        AssetInfo::Token {
            contract_addr: token_y.clone(),
        },
    ];

    // create pair
    let pair_addr = app.create_pair(asset_infos.clone()).unwrap();

    // provide liquidity
    // successfully provide liquidity for the exist pool

    app.approve_token(
        token_x_name,
        "addr0000",
        pair_addr.clone().as_str(),
        u128::MAX,
    )
    .unwrap();
    app.approve_token(
        token_y_name,
        "addr0000",
        pair_addr.clone().as_str(),
        u128::MAX,
    )
    .unwrap();
    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_x.clone(),
                },
                amount: Uint128::from(1000000000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_y.clone(),
                },
                amount: Uint128::from(100000000u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    let _res = app
        .execute(Addr::unchecked("addr0000"), pair_addr.clone(), &msg, &[])
        .unwrap();

    let msg = InstantiateMsg {
        factory_addr: app.factory_addr.clone(),
        factory_addr_v2: Addr::unchecked("addr0000_v2"),
        oraiswap_v3: app.v3_addr.clone(),
    };

    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));

    // we can just call .unwrap() to assert this was a success
    let router_addr = app
        .instantiate(code_id, Addr::unchecked("addr0000"), &msg, &[], "router")
        .unwrap();

    // first case: invalid route, can't swap
    let msg_swap = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![
            SwapOperation::OraiSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: token_x.clone(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: token_y.clone(),
                },
            },
            SwapOperation::SwapV3 {
                pool_key: pool_key.clone(),
                x_to_y: true,
            },
        ],
        minimum_receive: None,
        to: None,
        affiliates: None,
    };

    let error = app
        .execute(
            Addr::unchecked("addr0000"),
            token_x.clone(),
            &Cw20ExecuteMsg::Send {
                contract: router_addr.to_string(),
                amount: Uint128::new(1000000),
                msg: to_json_binary(&msg_swap).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert!(error.root_cause().to_string().contains("amount is zero"));

    // case 2: swap successful
    let msg_swap = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![
            SwapOperation::OraiSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: token_x.clone(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: token_y.clone(),
                },
            },
            SwapOperation::SwapV3 {
                pool_key,
                x_to_y: false,
            },
        ],
        minimum_receive: None,
        to: None,
        affiliates: None,
    };

    let mut balances_before = app.query_token_balances("addr0000").unwrap();
    balances_before.sort_by(|a, b| b.denom.cmp(&a.denom));
    assert_eq!(
        balances_before,
        vec![
            Coin {
                denom: token_y_name.to_string(),
                amount: Uint128::new(997402498276)
            },
            Coin {
                denom: token_x_name.to_string(),
                amount: Uint128::new(998500149965)
            }
        ]
    );

    app.execute(
        Addr::unchecked("addr0000"),
        token_x.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router_addr.to_string(),
            amount: Uint128::new(1000000),
            msg: to_json_binary(&msg_swap).unwrap(),
        },
        &[],
    )
    .unwrap();
    let mut balances_after = app.query_token_balances("addr0000").unwrap();
    balances_after.sort_by(|a, b| b.denom.cmp(&a.denom));
    assert_eq!(
        balances_after,
        vec![
            Coin {
                denom: token_y_name.to_string(),
                amount: Uint128::new(997402498276)
            },
            Coin {
                denom: token_x_name.to_string(),
                amount: Uint128::new(998499249465)
            }
        ]
    );
}

#[test]
fn test_affiliates() {
    let mut app = MockApp::new(&[(
        "addr0000",
        &[
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000000000000u128),
            },
            Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(1000000000000u128),
            },
        ],
    )]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    let token_x_name = "tokenx";
    let token_y_name = "tokeny";
    let token_x = app.create_token(token_x_name);
    let token_y = app.create_token(token_y_name);

    app.set_token_balances(&[("tokenx", &[("addr0000", 1000000000000u128)])])
        .unwrap();
    app.set_token_balances(&[("tokeny", &[("addr0000", 1000000000000u128)])])
        .unwrap();

    app.set_oracle_contract(Box::new(create_entry_points_testing!(oraiswap_oracle)));

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
        AssetInfo::Token {
            contract_addr: token_x.clone(),
        },
        AssetInfo::Token {
            contract_addr: token_y.clone(),
        },
    ];

    // create pair
    let pair_addr = app.create_pair(asset_infos.clone()).unwrap();

    // provide liquidity
    // successfully provide liquidity for the exist pool

    app.approve_token(
        token_x_name,
        "addr0000",
        pair_addr.clone().as_str(),
        u128::MAX,
    )
    .unwrap();
    app.approve_token(
        token_y_name,
        "addr0000",
        pair_addr.clone().as_str(),
        u128::MAX,
    )
    .unwrap();
    let msg = oraiswap::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_x.clone(),
                },
                amount: Uint128::from(1000000000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_y.clone(),
                },
                amount: Uint128::from(100000000u128),
            },
        ],
        slippage_tolerance: None,
        receiver: None,
    };

    let _res = app
        .execute(Addr::unchecked("addr0000"), pair_addr.clone(), &msg, &[])
        .unwrap();

    init_v3(
        &mut app,
        token_x.clone(),
        token_x_name,
        token_y.clone(),
        token_y_name,
    );

    let msg = InstantiateMsg {
        factory_addr: app.factory_addr.clone(),
        factory_addr_v2: Addr::unchecked("addr0000_v2"),
        oraiswap_v3: app.v3_addr.clone(),
    };

    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));

    // we can just call .unwrap() to assert this was a success
    let router_addr = app
        .instantiate(code_id, Addr::unchecked("addr0000"), &msg, &[], "router")
        .unwrap();

    // case 1: swap x to y => receive y
    let msg_swap = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![SwapOperation::OraiSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: token_x.clone(),
            },
            ask_asset_info: AssetInfo::Token {
                contract_addr: token_y.clone(),
            },
        }],
        minimum_receive: None,
        to: None,
        affiliates: Some(vec![
            Affiliate {
                basis_points_fee: Uint128::new(100), // 1%
                address: Addr::unchecked("affiliate_1"),
            },
            Affiliate {
                basis_points_fee: Uint128::new(1000), // 10%
                address: Addr::unchecked("affiliate_2"),
            },
        ]),
    };

    app.execute(
        Addr::unchecked("addr0000"),
        token_x.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router_addr.to_string(),
            amount: Uint128::new(1000000),
            msg: to_json_binary(&msg_swap).unwrap(),
        },
        &[],
    )
    .unwrap();
    let mut balances = app.query_token_balances("affiliate_1").unwrap();
    balances.sort_by(|a, b| b.denom.cmp(&a.denom));
    assert_eq!(
        balances,
        vec![
            Coin {
                denom: token_y_name.to_string(),
                amount: Uint128::new(995)
            },
            Coin {
                denom: token_x_name.to_string(),
                amount: Uint128::new(0)
            }
        ]
    );
    let mut balances = app.query_token_balances("affiliate_2").unwrap();
    balances.sort_by(|a, b| b.denom.cmp(&a.denom));
    assert_eq!(
        balances,
        vec![
            Coin {
                denom: token_y_name.to_string(),
                amount: Uint128::new(9950)
            },
            Coin {
                denom: token_x_name.to_string(),
                amount: Uint128::new(0)
            }
        ]
    );

    // swap y to x => receive x
    let msg_swap = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![SwapOperation::OraiSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: token_y.clone(),
            },
            ask_asset_info: AssetInfo::Token {
                contract_addr: token_x.clone(),
            },
        }],
        minimum_receive: None,
        to: None,
        affiliates: Some(vec![
            Affiliate {
                basis_points_fee: Uint128::new(100), // 1%
                address: Addr::unchecked("affiliate_1"),
            },
            Affiliate {
                basis_points_fee: Uint128::new(1000), // 10%
                address: Addr::unchecked("affiliate_2"),
            },
        ]),
    };

    app.execute(
        Addr::unchecked("addr0000"),
        token_x.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router_addr.to_string(),
            amount: Uint128::new(1000000),
            msg: to_json_binary(&msg_swap).unwrap(),
        },
        &[],
    )
    .unwrap();
    let mut balances = app.query_token_balances("affiliate_1").unwrap();
    balances.sort_by(|a, b| b.denom.cmp(&a.denom));
    assert_eq!(
        balances,
        vec![
            Coin {
                denom: token_y_name.to_string(),
                amount: Uint128::new(995)
            },
            Coin {
                denom: token_x_name.to_string(),
                amount: Uint128::new(10000)
            }
        ]
    );
    let mut balances = app.query_token_balances("affiliate_2").unwrap();
    balances.sort_by(|a, b| b.denom.cmp(&a.denom));
    assert_eq!(
        balances,
        vec![
            Coin {
                denom: token_y_name.to_string(),
                amount: Uint128::new(9950)
            },
            Coin {
                denom: token_x_name.to_string(),
                amount: Uint128::new(100000)
            }
        ]
    );
}
