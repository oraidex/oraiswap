use std::str::FromStr;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Addr, Coin, Decimal, Uint128};
use oraiswap::{
    asset::{Asset, AssetInfo, ORAI_DENOM},
    create_entry_points_testing,
    orderbook::{
        Cw20HookMsg, ExecuteMsg, InstantiateMsg, OrderDirection, QueryMsg,
        SimulateMarketOrderResponse,
    },
    testing::MockApp,
};

const REWARD_ADDR: &str = "orai16stq6f4pnrfpz75n9ujv6qg3czcfa4qyjux5en";

#[cw_serde]
enum OrderType {
    Market,
    Limit,
}

#[test]
fn stress_tests() {
    // case 1: only buy limit orders
    let senders: Vec<String> = vec![
        "addr000".to_string(),
        "addr001".to_string(),
        "addr002".to_string(),
        "addr003".to_string(),
    ];
    let directions = vec![OrderDirection::Buy];
    let order_types = vec![OrderType::Limit];
    let offer_amounts = vec![
        Uint128::from(100000u128),
        Uint128::from(1234554u128),
        Uint128::from(111111u128),
        Uint128::from(132232u128),
        Uint128::from(123123123u128),
        Uint128::from(21313123u128),
        Uint128::from(88448822u128),
        Uint128::from(4231231u128),
    ];
    let ask_amounts = vec![
        Uint128::from(100000u128),
        Uint128::from(763423232u128),
        Uint128::from(1312312u128),
        Uint128::from(424232u128),
        Uint128::from(1000320u128),
        Uint128::from(76323122u128),
        Uint128::from(132312312u128),
        Uint128::from(122312u128),
    ];
    let slippages = vec![
        None,
        Some(Decimal::from_str("0.1").unwrap()),
        Some(Decimal::from_str("0.01").unwrap()),
        Some(Decimal::from_str("0.2").unwrap()),
        None,
        Some(Decimal::from_str("0.05").unwrap()),
    ];
    simulate_submit_orders(
        1000,
        senders.clone(),
        directions.clone(),
        order_types.clone(),
        offer_amounts.clone(),
        ask_amounts.clone(),
        slippages.clone(),
    );

    // case 2: only sell limit orders
    let directions = vec![OrderDirection::Sell];
    simulate_submit_orders(
        1000,
        senders.clone(),
        directions.clone(),
        order_types.clone(),
        offer_amounts.clone(),
        ask_amounts.clone(),
        slippages.clone(),
    );

    // case 3: combine buy + sell orders
    let directions = vec![
        OrderDirection::Sell,
        OrderDirection::Sell,
        OrderDirection::Buy,
        OrderDirection::Sell,
        OrderDirection::Buy,
        OrderDirection::Buy,
        OrderDirection::Sell,
        OrderDirection::Buy,
    ];
    simulate_submit_orders(
        1000,
        senders.clone(),
        directions.clone(),
        order_types.clone(),
        offer_amounts.clone(),
        ask_amounts.clone(),
        slippages.clone(),
    );

    // case 4: combine buy + sell + market orders
    let order_types = vec![
        OrderType::Limit,
        OrderType::Limit,
        OrderType::Market,
        OrderType::Limit,
        OrderType::Market,
        OrderType::Market,
        OrderType::Market,
    ];
    simulate_submit_orders(
        1000,
        senders.clone(),
        directions.clone(),
        order_types.clone(),
        offer_amounts.clone(),
        ask_amounts.clone(),
        slippages.clone(),
    );
}

fn simulate_submit_orders(
    iterations: u32,
    senders: Vec<String>,
    directions: Vec<OrderDirection>,
    order_types: Vec<OrderType>,
    offer_amounts: Vec<Uint128>,
    ask_amounts: Vec<Uint128>,
    slippages: Vec<Option<Decimal>>,
) {
    let mut app = MockApp::new(&[]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    // init token
    let usdt_addr = app.set_token_balances(&[("usdt", &[])]).unwrap();

    for sender in senders.iter() {
        app.set_balances(&[(&ORAI_DENOM.to_string(), &[(sender, 10000000000000u128)])]);
        app.set_token_balances(&[("usdt", &[(sender, 10000000000000u128)])])
            .unwrap();
    }

    let msg = InstantiateMsg {
        name: None,
        version: None,
        admin: None,
        commission_rate: None,
        operator: None,
        reward_address: REWARD_ADDR.to_string(),
    };
    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));
    let orderbook_addr = app
        .instantiate(
            code_id,
            Addr::unchecked("addr0000"),
            &msg,
            &[],
            "limit order",
        )
        .unwrap();

    let orai_asset_info = AssetInfo::NativeToken {
        denom: ORAI_DENOM.to_string(),
    };
    let usdt_asset_info = AssetInfo::Token {
        contract_addr: usdt_addr[0].clone(),
    };
    // Create pair [orai, token_addrs[0]] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: orai_asset_info.clone(),
        quote_coin_info: usdt_asset_info.clone(),
        spread: Some(Decimal::from_ratio(1u128, 10u128)),
        min_quote_coin_amount: Uint128::from(10u128),
        refund_threshold: None,
        min_offer_to_fulfilled: None,
        min_ask_to_fulfilled: None,
    };

    let _res = app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &msg,
        &[],
    );

    for i in 0..iterations {
        let sender = senders[i as usize % senders.len()].clone();
        let direction = directions[i as usize % directions.len()].clone();
        let order_type = order_types[i as usize % order_types.len()].clone();
        let offer_amount = offer_amounts[i as usize % offer_amounts.len()].clone();
        let ask_amount = ask_amounts[i as usize % ask_amounts.len()].clone();
        let slippage = slippages[i as usize % slippages.len()].clone();

        match order_type {
            OrderType::Limit => match direction {
                OrderDirection::Buy => {
                    let msg = cw20::Cw20ExecuteMsg::Send {
                        contract: orderbook_addr.to_string(),
                        amount: offer_amount, // Fund must be equal to offer amount
                        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
                            direction: OrderDirection::Buy,
                            assets: [
                                Asset {
                                    info: orai_asset_info.clone(),
                                    amount: ask_amount,
                                },
                                Asset {
                                    info: usdt_asset_info.clone(),
                                    amount: offer_amount,
                                },
                            ],
                        })
                        .unwrap(),
                    };
                    app.execute(Addr::unchecked(sender), usdt_addr[0].clone(), &msg, &[])
                        .unwrap();
                }
                OrderDirection::Sell => {
                    let msg = ExecuteMsg::SubmitOrder {
                        direction: OrderDirection::Sell,
                        assets: [
                            Asset {
                                info: orai_asset_info.clone(),
                                amount: offer_amount,
                            },
                            Asset {
                                info: usdt_asset_info.clone(),
                                amount: ask_amount,
                            },
                        ],
                    };

                    app.execute(
                        Addr::unchecked(sender),
                        orderbook_addr.clone(),
                        &msg,
                        &[Coin {
                            denom: ORAI_DENOM.to_string(),
                            amount: offer_amount,
                        }],
                    )
                    .unwrap();
                }
            },
            OrderType::Market => match direction {
                OrderDirection::Buy => {
                    let msg = cw20::Cw20ExecuteMsg::Send {
                        contract: orderbook_addr.to_string(),
                        amount: offer_amount, // Fund must be equal to offer amount
                        msg: to_json_binary(&Cw20HookMsg::SubmitMarketOrder {
                            direction: OrderDirection::Buy,
                            asset_infos: [orai_asset_info.clone(), usdt_asset_info.clone()],
                            slippage,
                        })
                        .unwrap(),
                    };

                    let simulate = app
                        .query::<SimulateMarketOrderResponse, _>(
                            orderbook_addr.clone(),
                            &QueryMsg::SimulateMarketOrder {
                                direction: OrderDirection::Buy,
                                asset_infos: [orai_asset_info.clone(), usdt_asset_info.clone()],
                                slippage,
                                offer_amount,
                            },
                        )
                        .unwrap();

                    let res = app.execute(Addr::unchecked(sender), usdt_addr[0].clone(), &msg, &[]);
                    if simulate.receive == Uint128::zero() {
                        assert_eq!(res.is_err(), true)
                    } else {
                        res.unwrap();
                    }
                }
                OrderDirection::Sell => {
                    let msg = ExecuteMsg::SubmitMarketOrder {
                        direction: OrderDirection::Sell,
                        asset_infos: [orai_asset_info.clone(), usdt_asset_info.clone()],
                        slippage,
                    };

                    let simulate = app.query::<SimulateMarketOrderResponse, _>(
                        orderbook_addr.clone(),
                        &QueryMsg::SimulateMarketOrder {
                            direction: OrderDirection::Sell,
                            asset_infos: [orai_asset_info.clone(), usdt_asset_info.clone()],
                            slippage,
                            offer_amount,
                        },
                    );

                    if simulate.is_ok() {
                        let _res = app
                            .execute(
                                Addr::unchecked(sender),
                                orderbook_addr.clone(),
                                &msg,
                                &[Coin {
                                    denom: ORAI_DENOM.to_string(),
                                    amount: offer_amount,
                                }],
                            )
                            .unwrap();
                    }
                }
            },
        }
    }
}
