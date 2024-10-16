use std::str::FromStr;

use cosmwasm_std::testing::mock_dependencies;
use cosmwasm_std::{to_json_binary, Addr, Coin, Decimal, StdError, Uint128};
use oraiswap::create_entry_points_testing;
use oraiswap::testing::{AttributeUtil, MockApp, ATOM_DENOM};

use oraiswap::asset::{Asset, AssetInfo, AssetInfoRaw, ORAI_DENOM};
use oraiswap::orderbook::{
    ContractInfoResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, LastOrderIdResponse,
    OrderBookResponse, OrderBooksResponse, OrderDirection, OrderFilter, OrderResponse, OrderStatus,
    OrdersResponse, QueryMsg, SimulateMarketOrderResponse, TicksResponse,
};

use crate::jsonstr;
use crate::order::get_paid_and_quote_assets;
use crate::orderbook::OrderBook;
const USDT_DENOM: &str = "usdt";
const REWARD_ADDR: &str = "orai16stq6f4pnrfpz75n9ujv6qg3czcfa4qyjux5en";

fn basic_fixture() -> (MockApp, Addr) {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
    ]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    app.set_token_balances(&[("asset", &[("addr0000", 1000000000u128)])])
        .unwrap();

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

    // create order book for pair [orai, usdt]
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::from(10u128),
        refund_threshold: None,
        min_offer_to_fulfilled: None,
        min_ask_to_fulfilled: None,
    };
    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();
    (app, orderbook_addr)
}

#[test]
fn test_get_paid_and_quote_assets() {
    let deps = mock_dependencies();
    let asset_infos_raw = [
        AssetInfoRaw::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        AssetInfoRaw::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
    ];
    let assets = asset_infos_raw.clone().map(|info| Asset {
        info: info.to_normal(deps.as_ref().api).unwrap(),
        amount: Uint128::zero(),
    });
    let orderbook: OrderBook = OrderBook {
        base_coin_info: asset_infos_raw[0].clone(),
        quote_coin_info: asset_infos_raw[1].clone(),
        spread: None,
        min_quote_coin_amount: Uint128::zero(),
        refund_threshold: None,
        min_offer_to_fulfilled: None,
        min_ask_to_fulfilled: None,
    };
    // case 1: buy with base coin = asset_infos[0]
    let (paid_assets, quote_asset) = get_paid_and_quote_assets(
        deps.as_ref().api,
        &orderbook,
        assets.clone(),
        OrderDirection::Buy,
    )
    .unwrap();
    assert_eq!(paid_assets[0].info, assets[1].info);
    assert_eq!(paid_assets[1].info, assets[0].info);
    assert_eq!(quote_asset.info, assets[1].info);

    // case 2: sell with base coin = assets[0]
    let (paid_assets, quote_asset) = get_paid_and_quote_assets(
        deps.as_ref().api,
        &orderbook,
        assets.clone(),
        OrderDirection::Sell,
    )
    .unwrap();
    assert_eq!(paid_assets[0].info, assets[0].info);
    assert_eq!(paid_assets[1].info, assets[1].info);
    assert_eq!(quote_asset.info, assets[1].info);

    // case 3: buy with base coin = asset_infos[1]
    let mut reverse_assets = assets.clone();
    reverse_assets.reverse();
    let (paid_assets, quote_asset) = get_paid_and_quote_assets(
        deps.as_ref().api,
        &orderbook,
        reverse_assets.clone(),
        OrderDirection::Buy,
    )
    .unwrap();
    assert_eq!(paid_assets[0].info, reverse_assets[0].info);
    assert_eq!(paid_assets[1].info, reverse_assets[1].info);
    assert_eq!(quote_asset.info, reverse_assets[0].info);

    // case 4: sell with base coin = asset_infos[1]
    let mut reverse_assets = assets.clone();
    reverse_assets.reverse();
    let (paid_assets, quote_asset) = get_paid_and_quote_assets(
        deps.as_ref().api,
        &orderbook,
        reverse_assets.clone(),
        OrderDirection::Sell,
    )
    .unwrap();
    assert_eq!(paid_assets[0].info, reverse_assets[1].info);
    assert_eq!(paid_assets[1].info, reverse_assets[0].info);
    assert_eq!(quote_asset.info, reverse_assets[0].info);
}

#[test]
fn test_whitelist_trader() {
    let (mut app, orderbook_addr) = basic_fixture();
    // case 1: try to whitelist trader failed => unauthorized

    let update_msg = ExecuteMsg::WhitelistTrader {
        trader: Addr::unchecked("trader_1"),
    };
    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &update_msg,
            &[]
        )
        .is_err(),
        true
    );

    // case 2: good case, admin should be able to whitelist trader
    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &update_msg,
        &[],
    )
    .unwrap();
    let traders: Vec<String> = app
        .query(orderbook_addr.clone(), &QueryMsg::WhitelistedTraders {})
        .unwrap();
    assert_eq!(traders, vec!["trader_1".to_string()]);

    // add other trader
    let update_msg = ExecuteMsg::WhitelistTrader {
        trader: Addr::unchecked("trader_2"),
    };
    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &update_msg,
        &[],
    )
    .unwrap();
    let traders: Vec<String> = app
        .query(orderbook_addr.clone(), &QueryMsg::WhitelistedTraders {})
        .unwrap();
    assert_eq!(
        traders,
        vec!["trader_1".to_string(), "trader_2".to_string()]
    );
}

#[test]
fn test_remove_trader() {
    let (mut app, orderbook_addr) = basic_fixture();

    let update_msg = ExecuteMsg::WhitelistTrader {
        trader: Addr::unchecked("trader_1"),
    };

    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &update_msg,
        &[],
    )
    .unwrap();

    let update_msg = ExecuteMsg::WhitelistTrader {
        trader: Addr::unchecked("trader_2"),
    };
    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &update_msg,
        &[],
    )
    .unwrap();
    let traders: Vec<String> = app
        .query(orderbook_addr.clone(), &QueryMsg::WhitelistedTraders {})
        .unwrap();
    assert_eq!(
        traders,
        vec!["trader_1".to_string(), "trader_2".to_string()]
    );

    // remove failed, unauthorized
    let update_msg = ExecuteMsg::RemoveTrader {
        trader: Addr::unchecked("trader_1"),
    };
    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &update_msg,
            &[]
        )
        .is_err(),
        true
    );

    // remove successful
    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &update_msg,
        &[],
    )
    .unwrap();
    let traders: Vec<String> = app
        .query(orderbook_addr.clone(), &QueryMsg::WhitelistedTraders {})
        .unwrap();
    assert_eq!(traders, vec!["trader_2".to_string()]);
}

#[test]
fn test_withdraw_token() {
    let (mut app, orderbook_addr) = basic_fixture();
    // case 1: try to withdraw tokens using non-admin addr => unauthorized
    let asset_infos = [
        AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
    ];
    let asset = Asset {
        info: asset_infos.first().unwrap().clone(),
        amount: Uint128::from(10u128),
    };
    let update_msg = ExecuteMsg::WithdrawToken {
        asset: asset.clone(),
    };
    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &update_msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10u128), // deposit some tokens into the contract so we can mock withdrawing tokens
            }]
        )
        .is_err(),
        true
    );

    // case 2: good case, admin should be able to withdraw tokens
    let result = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &update_msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10u128), // deposit some tokens into the contract so we can mock withdrawing tokens
            }],
        )
        .unwrap();
    let info: ContractInfoResponse = app
        .query(orderbook_addr, &QueryMsg::ContractInfo {})
        .unwrap();
    for event in result.events {
        if event.ty.eq("wasm") {
            for attr in event.attributes.clone() {
                if attr.key.eq("action") {
                    assert_eq!(attr.value, "withdraw_token");
                }
                if attr.key.eq("token") {
                    assert_eq!(attr.value, asset.to_string());
                }
            }
        }
        if event.ty.eq("transfer") {
            for attr in event.attributes {
                if attr.key.eq("recipient") {
                    assert_eq!(attr.value, info.admin.to_string())
                }
            }
        }
    }
}

#[test]
fn test_pause_contract() {
    let (mut app, orderbook_addr) = basic_fixture();
    // case 1: try to paused contract using non-admin addr => unauthorized
    let asset_infos = [
        AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
    ];
    let asset = Asset {
        info: asset_infos.first().unwrap().clone(),
        amount: Uint128::from(10u128),
    };

    let pause_msg = ExecuteMsg::Pause {};
    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &pause_msg,
            &[]
        )
        .is_err(),
        true
    );

    // pause successful

    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &pause_msg,
        &[],
    )
    .unwrap();

    // after pause contract, can execute some funcs

    let update_msg = ExecuteMsg::WithdrawToken {
        asset: asset.clone(),
    };
    assert_eq!(
        app.execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &update_msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10u128), // deposit some tokens into the contract so we can mock withdrawing tokens
            }],
        )
        .is_err(),
        true
    );

    // unpause contract failed, non admin
    let pause_msg = ExecuteMsg::Unpause {};
    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &pause_msg,
            &[]
        )
        .is_err(),
        true
    );

    // unpause successful
    let pause_msg = ExecuteMsg::Unpause {};

    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &pause_msg,
        &[],
    )
    .unwrap();

    // after unpause, can execute contract

    let update_msg = ExecuteMsg::WithdrawToken {
        asset: asset.clone(),
    };

    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &update_msg,
        &[Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(10u128), // deposit some tokens into the contract so we can mock withdrawing tokens
        }],
    )
    .unwrap();
}

#[test]
fn test_update_admin() {
    let (mut app, orderbook_addr) = basic_fixture();

    let contract_info: ContractInfoResponse = app
        .query(orderbook_addr.clone(), &QueryMsg::ContractInfo {})
        .unwrap();

    let new_admin = Addr::unchecked("new_admin");
    let update_admin = ExecuteMsg::UpdateAdmin {
        admin: new_admin.clone(),
    };

    // case 1: try to update admin using non-admin addr => unauthorized

    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &update_admin,
            &[]
        )
        .is_err(),
        true
    );

    // update successful

    app.execute(
        contract_info.admin,
        orderbook_addr.clone(),
        &update_admin,
        &[],
    )
    .unwrap();

    let contract_info: ContractInfoResponse = app
        .query(orderbook_addr.clone(), &QueryMsg::ContractInfo {})
        .unwrap();
    assert_eq!(contract_info.admin, new_admin);
}

#[test]
fn test_update_operator() {
    let (mut app, orderbook_addr) = basic_fixture();

    let contract_info: ContractInfoResponse = app
        .query(orderbook_addr.clone(), &QueryMsg::ContractInfo {})
        .unwrap();

    let new_operator = "new_operator".to_string();
    let update_executor = ExecuteMsg::UpdateOperator {
        operator: Some(new_operator.clone()),
    };
    // case 1: try to update operator using non-admin addr => unauthorized

    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &update_executor,
            &[]
        )
        .is_err(),
        true
    );

    // update successful

    app.execute(
        contract_info.admin,
        orderbook_addr.clone(),
        &update_executor,
        &[],
    )
    .unwrap();

    let contract_info: ContractInfoResponse = app
        .query(orderbook_addr.clone(), &QueryMsg::ContractInfo {})
        .unwrap();
    assert_eq!(contract_info.operator, Some(Addr::unchecked(new_operator)));

    let update_executor = ExecuteMsg::UpdateOperator { operator: None };
    app.execute(
        contract_info.admin,
        orderbook_addr.clone(),
        &update_executor,
        &[],
    )
    .unwrap();

    let contract_info: ContractInfoResponse = app
        .query(orderbook_addr.clone(), &QueryMsg::ContractInfo {})
        .unwrap();
    assert_eq!(contract_info.operator, None);
}

#[test]
fn test_update_config() {
    let (mut app, orderbook_addr) = basic_fixture();

    let contract_info: ContractInfoResponse = app
        .query(orderbook_addr.clone(), &QueryMsg::ContractInfo {})
        .unwrap();

    let new_commission_rate = "0.01".to_string();
    let new_reward_address = Addr::unchecked("new_reward_address");
    let update_config = ExecuteMsg::UpdateConfig {
        reward_address: Some(new_reward_address.clone()),
        commission_rate: Some(new_commission_rate.clone()),
    };
    // case 1: try to update operator using non-admin addr => unauthorized

    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &update_config,
            &[]
        )
        .is_err(),
        true
    );

    // update successful

    app.execute(
        contract_info.admin,
        orderbook_addr.clone(),
        &update_config,
        &[],
    )
    .unwrap();

    let contract_info: ContractInfoResponse = app
        .query(orderbook_addr.clone(), &QueryMsg::ContractInfo {})
        .unwrap();
    assert_eq!(contract_info.commission_rate, new_commission_rate);
    assert_eq!(contract_info.reward_address, new_reward_address);
}

#[test]
fn test_crate_and_update_orderbook_data() {
    let (mut app, orderbook_addr) = basic_fixture();

    // create other orderbook pair failed, non admin
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: ATOM_DENOM.to_string(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::from(10u128),
        refund_threshold: None,
        min_offer_to_fulfilled: None,
        min_ask_to_fulfilled: None,
    };
    assert_eq!(
        app.execute(Addr::unchecked("theft"), orderbook_addr.clone(), &msg, &[],)
            .is_err(),
        true
    );

    // create other orderbook pair failed, spread > 1
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: ATOM_DENOM.to_string(),
        },
        spread: Some(Decimal::from_str("2").unwrap()),
        min_quote_coin_amount: Uint128::from(10u128),
        refund_threshold: None,
        min_offer_to_fulfilled: None,
        min_ask_to_fulfilled: None,
    };
    assert_eq!(
        app.execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .is_err(),
        true
    );

    // case 1: try to update orderbook spread with non-admin addr => unauthorized
    let asset_infos = [
        AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
    ];
    let update_msg = ExecuteMsg::UpdateOrderBookPair {
        asset_infos: asset_infos.clone(),
        spread: Some(Decimal::from_str("0.1").unwrap()),
        min_quote_coin_amount: None,
        refund_threshold: None,
        min_offer_to_fulfilled: None,
        min_ask_to_fulfilled: None,
    };
    assert_eq!(
        app.execute(
            Addr::unchecked("theft"),
            orderbook_addr.clone(),
            &update_msg,
            &[]
        )
        .is_err(),
        true
    );

    // update failed, spread > 1
    let update_msg = ExecuteMsg::UpdateOrderBookPair {
        asset_infos: asset_infos.clone(),
        spread: Some(Decimal::from_str("1.1").unwrap()),
        min_quote_coin_amount: None,
        refund_threshold: None,
        min_offer_to_fulfilled: None,
        min_ask_to_fulfilled: None,
    };
    assert_eq!(
        app.execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &update_msg,
            &[]
        )
        .is_err(),
        true
    );

    // case 2: good case, admin should update spread from None to something
    let spread = Decimal::from_str("0.1").unwrap();
    let update_msg = ExecuteMsg::UpdateOrderBookPair {
        asset_infos: asset_infos.clone(),
        spread: Some(spread),
        min_quote_coin_amount: Some(Uint128::from(100u128)),
        refund_threshold: Some(Uint128::from(100u128)),
        min_offer_to_fulfilled: Some(Uint128::from(10u128)),
        min_ask_to_fulfilled: Some(Uint128::from(10u128)),
    };
    app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &update_msg,
        &[],
    )
    .unwrap();
    let orderbook: OrderBookResponse = app
        .query(
            orderbook_addr.clone(),
            &QueryMsg::OrderBook {
                asset_infos: asset_infos.clone(),
            },
        )
        .unwrap();
    assert_eq!(orderbook.spread, Some(spread));
    // double check, make sure other fields are still the same
    assert_eq!(orderbook.base_coin_info, asset_infos[0]);
    assert_eq!(orderbook.quote_coin_info, asset_infos[1]);
    assert_eq!(orderbook.min_quote_coin_amount, Uint128::from(100u128));
    assert_eq!(orderbook.refund_threshold, Uint128::from(100u128));
    assert_eq!(orderbook.min_offer_to_fulfilled, Uint128::from(10u128));
    assert_eq!(orderbook.min_ask_to_fulfilled, Uint128::from(10u128));
}

#[test]
fn test_query_mid_price() {
    let (mut app, orderbook_addr) = basic_fixture();
    let res = app
        .query::<Decimal, _>(
            orderbook_addr.clone(),
            &QueryMsg::MidPrice {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
            },
        )
        .unwrap_err();

    assert!(res.to_string().contains("Cannot find a matched price"));

    // paid 300 usdt to get 150 orai -> 1 ORAI = 2 USD
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(150u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(300u128),
            },
        ],
    };

    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(300u128),
            }],
        )
        .unwrap();

    let res = app
        .query::<Decimal, _>(
            orderbook_addr.clone(),
            &QueryMsg::MidPrice {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
            },
        )
        .unwrap_err();
    assert!(res.to_string().contains("Cannot find a matched price"));
    // now we sell to get a different mid price
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(150u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
        ],
    };

    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(150u128),
            }],
        )
        .unwrap();

    let mid_price = app
        .query::<Decimal, _>(
            orderbook_addr.clone(),
            &QueryMsg::MidPrice {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
            },
        )
        .unwrap();
    assert_eq!(mid_price, Decimal::from_ratio(6u128, 1u128));
}

#[test]
fn submit_order() {
    let (mut app, orderbook_addr) = basic_fixture();

    let token_addr = app.get_token_addr("asset").unwrap();

    // Create an existed order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::zero(),
        refund_threshold: None,
        min_offer_to_fulfilled: None,
        min_ask_to_fulfilled: None,
    };
    let error = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap_err();

    assert!(error
        .root_cause()
        .to_string()
        .contains("Order book pair already exists"));

    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
        ],
    };

    // offer asset is null
    let error = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap_err();

    assert!(error
        .root_cause()
        .to_string()
        .contains("Native token balance mismatch between the argument and the transferred"));

    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(5u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(50u128),
            },
        ],
    };

    // Offer ammount 5 usdt (min 10 usdt) is too low
    let error = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(50u128),
            }],
        )
        .unwrap_err();

    assert!(error
        .root_cause()
        .to_string()
        .contains("Amount of usdt must be greater than 10"));

    // paid 150 usdt to get 150 orai'
    // order 1:
    // - side: buy
    // - price: 1
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(150u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(150u128),
            },
        ],
    };

    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(150u128),
            }],
        )
        .unwrap();

    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(150u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(0u128),
            },
        ],
    };

    // Asset must not be zero
    let error = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(0u128),
            }],
        )
        .unwrap_err();

    assert!(error
        .root_cause()
        .to_string()
        .contains("Cannot transfer empty coins amount"));

    let order_1 = OrderResponse {
        order_id: 1u64,
        bidder_addr: "addr0000".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(150u128),
            info: AssetInfo::NativeToken {
                denom: USDT_DENOM.to_string(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(150u128),
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        },
        filled_offer_amount: Uint128::zero(),
        filled_ask_amount: Uint128::zero(),
        direction: OrderDirection::Buy,
        status: OrderStatus::Open,
    };
    assert_eq!(
        order_1.clone(),
        app.query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 1,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
            }
        )
        .unwrap()
    );

    // order 2:
    // - side: buy
    // - price: 0.9000000
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(11111111u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(12345678u128),
            },
        ],
    };

    // paid 11111111 usdt to get 12345678 orai
    let res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(11111111u128),
            }],
        )
        .unwrap();
    println!("submit 2 {:?}", res);

    let order_2 = OrderResponse {
        order_id: 2u64,
        bidder_addr: "addr0000".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(11111111u128),
            info: AssetInfo::NativeToken {
                denom: USDT_DENOM.to_string(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(12345678u128),
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        },
        filled_offer_amount: Uint128::zero(),
        filled_ask_amount: Uint128::zero(),
        direction: OrderDirection::Buy,
        status: OrderStatus::Open,
    };
    assert_eq!(
        order_2.clone(),
        app.query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 2,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
            }
        )
        .unwrap()
    );

    // order 3:
    // side sell
    // price 0.2857
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(20000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(70000u128),
            },
        ],
    };

    // paid 70000 orai to get 20000 usdt
    let res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(70000u128),
            }],
        )
        .unwrap();
    println!("submit 3 {:?}", res);

    // after submit order3:
    // order 1 fulfilled
    // order 2: Partial Fill
    // order 3 full filled
    // matching process:
    // - order 3 matched 150 orai, 150 usdt with order 1
    // - order 3 matched (20000-150)=19850 usdt, 19850 / (11111111 / 12345678) = 22055 orai with order 2

    assert_eq!(
        app.query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 3,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
            }
        )
        .is_err(),
        true
    );

    let order_2 = OrderResponse {
        order_id: 2u64,
        bidder_addr: "addr0000".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(11111111u128),
            info: AssetInfo::NativeToken {
                denom: USDT_DENOM.to_string(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(12345678u128),
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        },
        filled_offer_amount: Uint128::from(19850u128),
        filled_ask_amount: Uint128::from(22055u128),
        direction: OrderDirection::Buy,
        status: OrderStatus::PartialFilled,
    };
    assert_eq!(
        order_2.clone(),
        app.query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 2,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
            }
        )
        .unwrap()
    );

    // create order book for pair [orai, token_addr]
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::Token {
            contract_addr: token_addr.clone(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::zero(),
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

    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1212121u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(2121212u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addr.clone(),
                    },
                    amount: Uint128::from(1212121u128),
                },
            ],
        })
        .unwrap(),
    };

    let _ = app
        .execute(Addr::unchecked("addr0000"), token_addr.clone(), &msg, &[])
        .unwrap();

    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addr.clone(),
                },
                amount: Uint128::from(1111111u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1234567u128),
            },
        ],
    };

    // paid 1234567 orai to get 1111111 token
    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1234567u128),
            }],
        )
        .unwrap();

    let order_4 = OrderResponse {
        order_id: 4u64,
        bidder_addr: "addr0000".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(1212121u128),
            info: AssetInfo::Token {
                contract_addr: token_addr.clone(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(2121212u128),
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        },
        filled_offer_amount: Uint128::zero(),
        filled_ask_amount: Uint128::zero(),
        direction: OrderDirection::Buy,
        status: OrderStatus::Open,
    };

    let order_5 = OrderResponse {
        order_id: 5u64,
        bidder_addr: "addr0000".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(1234567u128),
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(1111111u128),
            info: AssetInfo::Token {
                contract_addr: token_addr.clone(),
            },
        },
        filled_offer_amount: Uint128::zero(),
        filled_ask_amount: Uint128::zero(),
        direction: OrderDirection::Sell,
        status: OrderStatus::Open,
    };

    assert_eq!(
        order_4.clone(),
        app.query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 4,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::Token {
                        contract_addr: token_addr.clone(),
                    },
                ],
            }
        )
        .unwrap()
    );

    assert_eq!(
        order_5.clone(),
        app.query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 5,
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: token_addr.clone(),
                    },
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                ],
            }
        )
        .unwrap()
    );
    assert_eq!(
        app.query::<LastOrderIdResponse, _>(orderbook_addr.clone(), &QueryMsg::LastOrderId {})
            .unwrap(),
        LastOrderIdResponse { last_order_id: 5 }
    );
}

#[test]
fn cancel_order_native_token() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
    ]);

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

    // create order book for pair [orai, atom]
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::zero(),
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

    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(500000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(6666666u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(6666666u128),
            }],
        )
        .unwrap();

    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(456789u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(6666666u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(456789u128),
            }],
        )
        .unwrap();

    let msg = ExecuteMsg::CancelOrder {
        order_id: 1,
        asset_infos: [
            AssetInfo::NativeToken {
                denom: USDT_DENOM.to_string(),
            },
            AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        ],
    };

    // verfication failed
    let error = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap_err();

    assert!(error.root_cause().to_string().contains("Unauthorized"));

    let res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();
    assert_eq!(
        res.get_attributes(1),
        vec![
            ("action", "cancel_order"),
            ("pair", "orai - usdt"),
            ("order_id", "1"),
            ("direction", "Buy"),
            ("status", "Cancel"),
            ("bidder_addr", "addr0000"),
            ("offer_amount", "6666666"),
            ("ask_amount", "500000"),
            ("bidder_refund", &format!("6666666{}", USDT_DENOM)),
        ]
    );

    let mut address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    let mut address1_balances = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    println!("round 1 - address0_balances: {:?}", address0_balances);
    println!("round 1 - address1_balances: {:?}", address1_balances);

    let mut expected_balances: Vec<Coin> = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        },
    ]
    .to_vec();
    assert_eq!(address0_balances, expected_balances,);
    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(999543211u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        },
    ]
    .to_vec();
    assert_eq!(address1_balances, expected_balances,);

    // failed no order exists
    let error = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap_err();

    assert!(error.root_cause().to_string().contains("Order not found"));

    let msg = ExecuteMsg::CancelOrder {
        order_id: 2,
        asset_infos: [
            AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
            AssetInfo::NativeToken {
                denom: USDT_DENOM.to_string(),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();

    address1_balances = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    println!("round 2 - address1_balances: {:?}", address1_balances);
    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        },
    ]
    .to_vec();
    assert_eq!(address1_balances, expected_balances,);

    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1234560u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1234560u128),
            }],
        )
        .unwrap();

    address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    println!("round 3 - address0_balances: {:?}", address0_balances);
    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(998765440u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        },
    ]
    .to_vec();
    assert_eq!(address0_balances, expected_balances,);

    let msg = ExecuteMsg::CancelOrder {
        order_id: 3,
        asset_infos: [
            AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
            AssetInfo::NativeToken {
                denom: USDT_DENOM.to_string(),
            },
        ],
    };

    let res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();
    assert_eq!(
        res.get_attributes(1),
        vec![
            ("action", "cancel_order"),
            ("pair", "orai - usdt"),
            ("order_id", "3"),
            ("direction", "Sell"),
            ("status", "Cancel"),
            ("bidder_addr", "addr0000"),
            ("offer_amount", "1234560"),
            ("ask_amount", "1000000"),
            ("bidder_refund", &format!("1234560{}", ORAI_DENOM)),
        ]
    );
    address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    println!("round 4 - address0_balances: {:?}", address0_balances);
    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        },
    ]
    .to_vec();
    assert_eq!(address0_balances, expected_balances,);
}

#[test]
fn cancel_order_token() {
    let mut app = MockApp::new(&[(
        "addr0000",
        &[Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(1000000000u128),
        }],
    )]);
    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    let token_addrs = app
        .set_token_balances(&[
            (
                "assetA",
                &[("addr0000", 1000000000u128), ("addr0001", 1000000000u128)],
            ),
            (
                "assetB",
                &[("addr0000", 1000000000u128), ("addr0001", 1000000000u128)],
            ),
        ])
        .unwrap();

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

    // create order book for pair [token_addrs[1], token_addrs[0]]
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::Token {
            contract_addr: token_addrs[1].clone(),
        },
        quote_coin_info: AssetInfo::Token {
            contract_addr: token_addrs[0].clone(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::zero(),
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

    // create order book for pair [orai, token_addrs[1]]
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::Token {
            contract_addr: token_addrs[1].clone(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::zero(),
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

    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1234567u128), // Fund must be equal to offer amount
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1234567u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(4567890u128),
                },
            ],
        })
        .unwrap(),
    };

    let msg2 = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(3333335u128), // Fund must be equal to offer amount
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Sell,
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1212121u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(3333335u128),
                },
            ],
        })
        .unwrap(),
    };

    let msg3 = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(3333336u128), // Fund must be equal to offer amount
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Sell,
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1212121u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(3333335u128),
                },
            ],
        })
        .unwrap(),
    };

    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[1].clone(),
            &msg2,
            &[],
        )
        .unwrap();

    // provided and paid asset are different
    let error = app
        .execute(
            Addr::unchecked("addr0001"),
            token_addrs[1].clone(),
            &msg3,
            &[],
        )
        .unwrap_err();
    assert!(error.root_cause().to_string().contains("Invalid funds"));

    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1223344u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Sell,
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(1223344u128),
                },
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(2334455u128),
                },
            ],
        })
        .unwrap(),
    };

    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[1].clone(),
            &msg,
            &[],
        )
        .unwrap();

    let msg = ExecuteMsg::CancelOrder {
        order_id: 1,
        asset_infos: [
            AssetInfo::Token {
                contract_addr: token_addrs[0].clone(),
            },
            AssetInfo::Token {
                contract_addr: token_addrs[1].clone(),
            },
        ],
    };

    // failed verfication failed
    let error = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert!(error.root_cause().to_string().contains("Unauthorized"));

    let res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();

    assert_eq!(
        res.get_attributes(1),
        vec![
            ("action", "cancel_order"),
            ("pair", "contract1 - contract0"),
            ("order_id", "1"),
            ("direction", "Buy"),
            ("status", "Cancel"),
            ("bidder_addr", "addr0000"),
            ("offer_amount", "1234567"),
            ("ask_amount", "4567890"),
            ("bidder_refund", &format!("1234567{}", token_addrs[0])),
        ]
    );

    let msg = ExecuteMsg::CancelOrder {
        order_id: 2,
        asset_infos: [
            AssetInfo::Token {
                contract_addr: token_addrs[1].clone(),
            },
            AssetInfo::Token {
                contract_addr: token_addrs[0].clone(),
            },
        ],
    };

    let res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();

    assert_eq!(
        res.get_attributes(1),
        vec![
            ("action", "cancel_order"),
            ("pair", "contract1 - contract0"),
            ("order_id", "2"),
            ("direction", "Sell"),
            ("status", "Cancel"),
            ("bidder_addr", "addr0000"),
            ("offer_amount", "3333335"),
            ("ask_amount", "1212121"),
            ("bidder_refund", &format!("3333335{}", token_addrs[1])),
        ]
    );

    // failed no order exists
    let error = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert!(error.root_cause().to_string().contains("Order not found"));
}

#[test]
fn execute_pair_native_token() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
        (
            "addr0002",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
    ]);

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

    // Create pair [orai, usdt] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
        spread: None,
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

    /* <----------------------------------- order 1 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 2 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(9700u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 3 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(13000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(14000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(13000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 4 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(5000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
        ],
    };

    // offer usdt, ask for orai
    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(5000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 5 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(4400u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(8800u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(4400u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 6 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(7000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(14000u128),
            },
        ],
    };

    // offer orai, ask for atom
    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(7000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 7 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(2000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 8 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1200u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1200u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 9 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(5000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 10 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(7000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(6789u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(7000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 11 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 12 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1600u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 13 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1500u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 14 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1600u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1600u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 15 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 16 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(9700u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 17 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(14000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(13000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(13000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 18 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(5000u128),
            },
        ],
    };

    // offer usdt, ask for orai
    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(5000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 19 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(8800u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(4400u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(4400u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 20 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(14000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(7000u128),
            },
        ],
    };

    // offer orai, ask for atom
    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(7000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 21 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(2000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 22 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1200u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1200u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 23 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(5000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 24 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(7000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(6789u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(7000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 25 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 26 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1600u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 27 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1500u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 28 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1600u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1600u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 29 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(2000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 30 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1200u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1200u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 31 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1200u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1200u128),
            }],
        )
        .unwrap();

    let address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    let address1_balances = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    let address2_balances = app.query_all_balances(Addr::unchecked("addr0002")).unwrap();
    let reward_balances = app
        .query_all_balances(Addr::unchecked(
            "orai16stq6f4pnrfpz75n9ujv6qg3czcfa4qyjux5en",
        ))
        .unwrap();
    let spread_balances = app
        .query_all_balances(Addr::unchecked(
            "orai139tjpfj0h6ld3wff7v2x92ntdewungfss0ml3n",
        ))
        .unwrap();

    println!("round 1 - address0's balances: {:?}", address0_balances);
    println!("round 1 - address1's balances: {:?}", address1_balances);
    println!("round 1 - address2's balances: {:?}", address2_balances);
    println!(
        "round 1 - reward_balances's balances: {:?}",
        reward_balances
    );
    println!(
        "round 1 - spread_balances's balances: {:?}\n\n",
        spread_balances
    );

    let expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(969990u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(976045u128),
        },
    ]
    .to_vec();
    assert_eq!(address0_balances, expected_balances,);
    let expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(978100u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(968842u128),
        },
    ]
    .to_vec();
    assert_eq!(address1_balances, expected_balances);
    let expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(1000000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(981200u128),
        },
    ]
    .to_vec();
    assert_eq!(address2_balances, expected_balances);
}

#[test]
fn execute_pair_cw20_token() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000000u128),
            }],
        ),
        (
            "addr0001",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000000u128),
            }],
        ),
        (
            "addr0002",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000000u128),
            }],
        ),
    ]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    let token_addrs = app
        .set_token_balances(&[
            (
                "usdt",
                &[
                    ("addr0000", 1000000u128),
                    ("addr0001", 1000000u128),
                    ("addr0002", 1000000u128),
                ],
            ),
            (
                "uusd",
                &[
                    ("addr0000", 1000000u128),
                    ("addr0001", 1000000u128),
                    ("addr0002", 1000000u128),
                ],
            ),
        ])
        .unwrap();

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

    // Create pair [orai, token_addrs[0]] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::Token {
            contract_addr: token_addrs[0].clone(),
        },
        spread: None,
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

    // submit order failed, invalid funds
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(13000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(13000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(13000u128),
                },
            ],
        })
        .unwrap(),
    };

    assert_eq!(
        app.execute(
            Addr::unchecked("addr0001"),
            token_addrs[1].clone(),
            &msg,
            &[],
        )
        .is_err(),
        true
    );

    //  submit order failed, TooSmallQuoteAsset
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(9u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(10u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(9u128),
                },
            ],
        })
        .unwrap(),
    };

    assert_eq!(
        app.execute(
            Addr::unchecked("addr0001"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .is_err(),
        true
    );

    /* <----------------------------------- order 1 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(10000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 2 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(9700u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 3 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(13000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(13000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(13000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app.execute(
        Addr::unchecked("addr0001"),
        token_addrs[0].clone(),
        &msg,
        &[],
    );

    /* <----------------------------------- order 4 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(5000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(10000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(5000u128),
                },
            ],
        })
        .unwrap(),
    };

    // offer usdt, ask for orai
    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 5 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(4400u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(8800u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(4400u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 6 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(7000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(14000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(7000u128),
                },
            ],
        })
        .unwrap(),
    };

    // offer orai, ask for usdt
    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 7 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(2000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(2000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 8 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1200u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(1500u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1200u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 9 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(10000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(5000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(10000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 10 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(7000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(6789u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(7000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 11 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(1500u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 12 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(1600u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 13 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1500u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 14 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1600u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1600u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 15 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(10000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 16 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(9700u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 17 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(13000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(14000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(13000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 18 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(5000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(10000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(5000u128),
                },
            ],
        })
        .unwrap(),
    };

    // offer usdt, ask for orai
    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 19 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(4400u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(8800u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(4400u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 20 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(7000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(14000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(7000u128),
                },
            ],
        })
        .unwrap(),
    };

    // offer cw20 usdt, ask for orai
    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 21 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(2000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(2000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 22 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1200u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(1500u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1200u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 23 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(10000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(5000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(10000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 24 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(7000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(6789u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(7000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 25 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(1500u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 26 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(1600u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 27 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1500u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1500u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 28 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1600u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(1000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1600u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 29 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(2000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
                amount: Uint128::from(2000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(2000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 30 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1200u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(1500u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1200u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    /* <----------------------------------- order 31 -----------------------------------> */
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(1200u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(1500u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1200u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    let address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    let address1_balances = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    let address2_balances = app.query_all_balances(Addr::unchecked("addr0002")).unwrap();
    println!("round 0 - address0's balances: {:?}", address0_balances);
    println!("round 0 - address1's balances: {:?}", address1_balances);
    println!("round 0 - address2's balances: {:?}\n\n", address2_balances);

    let expected_balances = [Coin {
        denom: ORAI_DENOM.to_string(),
        amount: Uint128::from(972688u128),
    }]
    .to_vec();
    assert_eq!(address0_balances, expected_balances,);
    let expected_balances = [Coin {
        denom: ORAI_DENOM.to_string(),
        amount: Uint128::from(988937u128),
    }]
    .to_vec();
    assert_eq!(address1_balances, expected_balances,);
    let expected_balances = [Coin {
        denom: ORAI_DENOM.to_string(),
        amount: Uint128::from(1000000u128),
    }]
    .to_vec();
    assert_eq!(address2_balances, expected_balances,);
}

#[test]
fn simple_matching_test() {
    let mut app: MockApp = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(10000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(10000000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(10000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(10000000000u128),
                },
            ],
        ),
        (
            "addr0002",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(10000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(10000000000u128),
                },
            ],
        ),
    ]);

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

    // Create pair [orai, usdt] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
        spread: Some(Decimal::percent(1)),
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

    /* <----------------------------------- order 1 -----------------------------------> */
    // addr0 sell at price 7.51234
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(75123400u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 2 -----------------------------------> */
    // addr2 buy at price 7.52
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(100000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(752000000u128),
            },
        ],
    };

    // offer usdt, ask for orai
    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(752000000u128),
            }],
        )
        .unwrap();

    // after submit order 2:
    // - order 1 fulfilled, addr0 receive 75123400 - 75123400 * 0.001 = 75048276 (after round number) usdt
    // - order 2 partial filled, addr2 receive 10000000 - 10000000 * 0.001  = 9990000 orai

    let mut address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    let mut address1_balances = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    let mut address2_balances = app.query_all_balances(Addr::unchecked("addr0002")).unwrap();
    println!("round 0 - address0's balances: {:?}", address0_balances);
    println!("round 0 - address1's balances: {:?}", address1_balances);
    println!("round 0 - address2's balances: {:?}\n\n", address2_balances);

    let mut expected_balances: Vec<Coin> = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(9990000000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(10075048277u128),
        },
    ]
    .to_vec();
    assert_eq!(address0_balances, expected_balances,);

    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(10009990000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(9248000000u128),
        },
    ]
    .to_vec();
    assert_eq!(address2_balances, expected_balances);

    /* <----------------------------------- order 3 -----------------------------------> */
    // addr0 sell at price 7.51234
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(100000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(751234u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(100000u128),
            }],
        )
        .unwrap();

    // after submit order 3:
    // - order 3 fulfilled, addr0 receive 751234 - 751234 * 0.001 = 750482 (after round number) usdt
    // - order 2 partial filled, addr2 receive 751234/7.52 * 0.999 =  99798 orai

    address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    address1_balances = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    address2_balances = app.query_all_balances(Addr::unchecked("addr0002")).unwrap();
    println!("round 1 - address0's balances: {:?}", address0_balances);
    println!("round 1 - address1's balances: {:?}", address1_balances);
    println!("round 1 - address2's balances: {:?}\n\n", address2_balances);

    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(9989900000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(10075798760u128),
        },
    ]
    .to_vec();
    assert_eq!(address0_balances, expected_balances);
    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(10010089799u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(9248000000u128),
        },
    ]
    .to_vec();
    assert_eq!(address2_balances, expected_balances);
}

#[test]
fn reward_to_executor_test() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
    ]);

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

    // Create pair [orai, usdt] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
        spread: Some(Decimal::percent(10)),
        min_quote_coin_amount: Uint128::from(10000u128),
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

    /* <----------------------------------- order 1 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(103000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(618000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(103000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 2 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(610000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(100000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(100000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 3 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(100000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(600000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(600000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 4 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(610000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(100000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(610000u128),
            }],
        )
        .unwrap();

    let address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    let address1_balances = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    println!("round 0 - address0's balances: {:?}", address0_balances);
    println!("round 0 - address1's balances: {:?}\n\n", address1_balances);

    let mut expected_balances: Vec<Coin> = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(1001208491u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(999797000u128),
        },
    ]
    .to_vec();
    assert_eq!(address0_balances, expected_balances,);
    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(998790000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(1000199800u128),
        },
    ]
    .to_vec();
    assert_eq!(address1_balances, expected_balances,);
}

#[test]
fn whitelist_trader_with_zero_fee() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
    ]);

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

    // Create pair [orai, usdt] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
        spread: Some(Decimal::percent(10)),
        min_quote_coin_amount: Uint128::from(10000u128),
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

    // whitelist trader
    let msg = ExecuteMsg::WhitelistTrader {
        trader: Addr::unchecked("addr0000"),
    };
    let _res = app.execute(
        Addr::unchecked("addr0000"),
        orderbook_addr.clone(),
        &msg,
        &[],
    );

    /* <----------------------------------- order 1 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(103000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(618000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(103000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 2 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(610000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(100000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(100000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 3 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(100000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(600000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(600000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 4 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(610000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(100000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(610000u128),
            }],
        )
        .unwrap();

    let address0_balances = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    let address1_balances = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    println!("round 0 - address0's balances: {:?}", address0_balances);
    println!("round 0 - address1's balances: {:?}\n\n", address1_balances);

    let mut expected_balances: Vec<Coin> = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(1001209700u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(999797000u128),
        },
    ]
    .to_vec();
    assert_eq!(address0_balances, expected_balances,);
    expected_balances = [
        Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(998790000u128),
        },
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(1000199800u128),
        },
    ]
    .to_vec();
    assert_eq!(address1_balances, expected_balances,);
}

fn mock_basic_query_data() -> (MockApp, Addr) {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
        (
            "addr0002",
            &[
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: USDT_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
    ]);

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

    // Create pair [orai, usdt] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: USDT_DENOM.to_string(),
        },
        spread: Some(Decimal::percent(10)),
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
    (app, orderbook_addr)
}

#[test]
fn remove_orderbook_pair() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ATOM_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ATOM_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
        (
            "addr0002",
            &[
                Coin {
                    denom: ATOM_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000u128),
                },
            ],
        ),
    ]);

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

    // Create pair [orai, atom] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ATOM_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::zero(),
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

    /* <----------------------------------- order 1 -----------------------------------> */
    // sell at price 2
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: Uint128::from(11111u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(22222u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(11111u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 2 -----------------------------------> */
    // sell at price 1.5
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: Uint128::from(12222u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(18333u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(12222u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 3 -----------------------------------> */
    // buy at price 1
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: Uint128::from(13000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(13000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(13000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 4 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: Uint128::from(1900u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1499u128),
            },
        ],
    };

    // offer orai, ask for atom
    let _res = app
        .execute(
            Addr::unchecked("addr0002"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1499u128),
            }],
        )
        .unwrap();

    let order_3 = OrderResponse {
        order_id: 3u64,
        bidder_addr: "addr0001".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(13000u128),
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(13000u128),
            info: AssetInfo::NativeToken {
                denom: ATOM_DENOM.to_string(),
            },
        },
        filled_offer_amount: Uint128::zero(),
        filled_ask_amount: Uint128::zero(),
        direction: OrderDirection::Buy,
        status: OrderStatus::Open,
    };

    assert_eq!(
        order_3,
        app.query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 3,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                ],
            }
        )
        .unwrap()
    );

    // remove order book for pair [orai, atom]
    let msg = ExecuteMsg::RemoveOrderBookPair {
        asset_infos: [
            AssetInfo::NativeToken {
                denom: ATOM_DENOM.to_string(),
            },
            AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        ],
    };

    let res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();

    println!("remove order book pair res: {:?}", res);

    let res = app
        .query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                ],
                direction: None,
                filter: OrderFilter::None,
                start_after: None,
                limit: None,
                order_by: None,
            },
        )
        .unwrap_err();
    assert_eq!(
        res,
        StdError::GenericErr {
            msg: "Querier contract error: oraiswap_orderbook::orderbook::OrderBook not found"
                .to_string()
        }
    );
    let res = app
        .query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 3,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                ],
            },
        )
        .unwrap_err();
    assert_eq!(
        res,
        StdError::GenericErr {
            msg: "Querier contract error: oraiswap_orderbook::orderbook::OrderBook not found"
                .to_string()
        }
    );
}

#[test]
fn orders_querier() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[
                Coin {
                    denom: ATOM_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
        (
            "addr0001",
            &[
                Coin {
                    denom: ATOM_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
                Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(1000000000u128),
                },
            ],
        ),
    ]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    let token_addrs = app
        .set_token_balances(&[
            (
                "assetA",
                &[("addr0000", 1000000000u128), ("addr0001", 1000000000u128)],
            ),
            (
                "assetB",
                &[("addr0000", 1000000000u128), ("addr0001", 1000000000u128)],
            ),
        ])
        .unwrap();

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

    // create order book for pair [orai, atom]
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ATOM_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        spread: Some(Decimal::percent(10)),
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

    // create order book for pair [token_addrs[1], token_addrs[0]]
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::Token {
            contract_addr: token_addrs[1].clone(),
        },
        quote_coin_info: AssetInfo::Token {
            contract_addr: token_addrs[0].clone(),
        },
        spread: None,
        min_quote_coin_amount: Uint128::zero(),
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

    // query orderbooks
    let res = app
        .query::<OrderBookResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::OrderBook {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                ],
            },
        )
        .unwrap();
    println!("[LOG] 1st orderbooks :{}", jsonstr!(res));

    // query all orderbooks
    let res = app
        .query::<OrderBooksResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::OrderBooks {
                start_after: None,
                limit: None,
                order_by: None,
            },
        )
        .unwrap();

    println!("orderbooks :{}", jsonstr!(res));

    // order 1
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000000u128),
            }],
        )
        .unwrap();

    // user sends token therefore no need to set allowance for limit order contract
    // order 2 buy: price 1
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::from(1000000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Buy,
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(1000000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(1000000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    // order 3: sell: price 2
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::from(1000000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Sell,
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(2000000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(1000000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            token_addrs[1].clone(),
            &msg,
            &[],
        )
        .unwrap();

    // order 4 sell: price 2.1
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::from(1000000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
            direction: OrderDirection::Sell,
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(1000000u128),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(2100000u128),
                },
            ],
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            token_addrs[1].clone(),
            &msg,
            &[],
        )
        .unwrap();

    let order_1 = OrderResponse {
        order_id: 1u64,
        bidder_addr: "addr0000".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(1000000u128),
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(1000000u128),
            info: AssetInfo::NativeToken {
                denom: ATOM_DENOM.to_string(),
            },
        },
        filled_offer_amount: Uint128::zero(),
        filled_ask_amount: Uint128::zero(),
        direction: OrderDirection::Buy,
        status: OrderStatus::Open,
    };

    let order_2 = OrderResponse {
        order_id: 2u64,
        bidder_addr: "addr0000".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(1000000u128),
            info: AssetInfo::Token {
                contract_addr: token_addrs[0].clone(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(1000000u128),
            info: AssetInfo::Token {
                contract_addr: token_addrs[1].clone(),
            },
        },
        filled_offer_amount: Uint128::zero(),
        filled_ask_amount: Uint128::zero(),
        direction: OrderDirection::Buy,
        status: OrderStatus::Open,
    };

    let all_order = OrdersResponse {
        orders: [
            OrderResponse {
                order_id: 4u64,
                direction: OrderDirection::Sell,
                bidder_addr: "addr0001".to_string(),
                offer_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(1000000u128),
                },
                ask_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(2100000u128),
                },
                filled_offer_amount: Uint128::zero(),
                filled_ask_amount: Uint128::zero(),
                status: OrderStatus::Open,
            },
            OrderResponse {
                order_id: 3u64,
                direction: OrderDirection::Sell,
                bidder_addr: "addr0001".to_string(),
                offer_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    amount: Uint128::from(1000000u128),
                },
                ask_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(2000000u128),
                },
                filled_offer_amount: Uint128::zero(),
                filled_ask_amount: Uint128::zero(),
                status: OrderStatus::Open,
            },
            OrderResponse {
                order_id: 2u64,
                direction: OrderDirection::Buy,
                bidder_addr: "addr0000".to_string(),
                offer_asset: Asset {
                    amount: Uint128::from(1000000u128),
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                },
                ask_asset: Asset {
                    amount: Uint128::from(1000000u128),
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                },
                filled_offer_amount: Uint128::zero(),
                filled_ask_amount: Uint128::zero(),
                status: OrderStatus::Open,
            },
        ]
        .to_vec(),
    };

    assert_eq!(
        OrdersResponse {
            orders: vec![order_2.clone(),],
        },
        app.query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                ],
                direction: None,
                filter: OrderFilter::Bidder("addr0000".to_string()),
                start_after: None,
                limit: None,
                order_by: Some(1),
            }
        )
        .unwrap()
    );

    let test = app
        .query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                ],
                direction: Some(OrderDirection::Buy),
                filter: OrderFilter::None,
                start_after: None,
                limit: None,
                order_by: None,
            },
        )
        .unwrap();
    println!("[LOG] [1] - query all buy order: {}", jsonstr!(test));

    let test = app
        .query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                ],
                direction: Some(OrderDirection::Sell), //None
                filter: OrderFilter::None,
                start_after: None,
                limit: None,
                order_by: None,
            },
        )
        .unwrap();
    println!("[LOG] [2] - query all sell order: {}", jsonstr!(test));

    let test = app
        .query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                ],
                direction: None,
                filter: OrderFilter::None,
                start_after: None,
                limit: None,
                order_by: None,
            },
        )
        .unwrap();
    println!("[LOG] [3] - query all order: {}", jsonstr!(test));

    assert_eq!(
        OrdersResponse {
            orders: vec![order_1.clone()],
        },
        app.query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                ],
                direction: None,
                filter: OrderFilter::None,
                start_after: None,
                limit: None,
                order_by: Some(1),
            }
        )
        .unwrap()
    );

    // DESC test
    assert_eq!(
        all_order.clone(),
        app.query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: token_addrs[1].clone(),
                    },
                    AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                ],
                direction: None,
                filter: OrderFilter::None,
                start_after: None,
                limit: None,
                order_by: Some(2),
            }
        )
        .unwrap()
    );

    // different bidder
    assert_eq!(
        OrdersResponse { orders: vec![] },
        app.query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                ],
                direction: None,
                filter: OrderFilter::Bidder("addr0001".to_string()),
                start_after: None,
                limit: None,
                order_by: None,
            }
        )
        .unwrap()
    );

    // start after DESC
    assert_eq!(
        OrdersResponse {
            orders: vec![order_1],
        },
        app.query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                ],
                direction: None,
                filter: OrderFilter::None,
                start_after: Some(2u64),
                limit: None,
                order_by: Some(2),
            }
        )
        .unwrap()
    );

    // start after ASC
    assert_eq!(
        OrdersResponse { orders: vec![] },
        app.query::<OrdersResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Orders {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                ],
                direction: None,
                filter: OrderFilter::None,
                start_after: Some(1u64),
                limit: None,
                order_by: Some(1),
            }
        )
        .unwrap()
    );

    // query all ticks
    let res = app
        .query::<TicksResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Ticks {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: ATOM_DENOM.to_string(),
                    },
                ],
                direction: OrderDirection::Buy,
                start_after: None,
                end: None,
                limit: None,
                order_by: Some(1),
            },
        )
        .unwrap();

    for tick in res.ticks {
        let res = app
            .query::<OrdersResponse, _>(
                orderbook_addr.clone(),
                &QueryMsg::Orders {
                    asset_infos: [
                        AssetInfo::NativeToken {
                            denom: ORAI_DENOM.to_string(),
                        },
                        AssetInfo::NativeToken {
                            denom: ATOM_DENOM.to_string(),
                        },
                    ],
                    direction: None,
                    filter: OrderFilter::Price(tick.price),
                    start_after: None,
                    limit: None,
                    order_by: Some(1),
                },
            )
            .unwrap();
        println!("{:?}", res);
    }
}

#[test]
fn test_query_ticks_start_after() {
    let (mut app, orderbook_addr) = mock_basic_query_data();

    /* <----------------------------------- order 1 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(20000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 2 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(30000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    let result = app
        .query::<TicksResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Ticks {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
                direction: OrderDirection::Sell,
                start_after: Some(Decimal::from_str("3").unwrap()),
                end: None,
                limit: None,
                order_by: Some(2),
            },
        )
        .unwrap();
    assert_eq!(result.ticks.len(), 1);

    let result = app
        .query::<TicksResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Ticks {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
                direction: OrderDirection::Sell,
                start_after: Some(Decimal::from_str("2").unwrap()),
                end: None,
                limit: None,
                order_by: Some(1),
            },
        )
        .unwrap();
    assert_eq!(result.ticks.len(), 1);
}

#[test]
fn test_unwrap_default_check_sub_uint128() {
    let result = Uint128::from(0u64)
        .checked_sub(Uint128::from(1u64))
        .unwrap_or_default();
    assert_eq!(result, Uint128::from(0u64));
}

#[test]
fn test_query_ticks_with_end() {
    let (mut app, orderbook_addr) = mock_basic_query_data();

    /* <----------------------------------- order 1 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(20000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    /* <----------------------------------- order 2 -----------------------------------> */
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(30000u128),
            },
        ],
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000u128),
            }],
        )
        .unwrap();

    let result = app
        .query::<TicksResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Ticks {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
                direction: OrderDirection::Sell,
                start_after: Some(Decimal::from_str("3").unwrap()),
                end: Some(Decimal::from_str("2").unwrap()),
                limit: None,
                order_by: Some(2),
            },
        )
        .unwrap();
    assert_eq!(result.ticks.len(), 1);
    assert_eq!(result.ticks[0].price, Decimal::from_str("2").unwrap());

    let result = app
        .query::<TicksResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Ticks {
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
                direction: OrderDirection::Sell,
                start_after: Some(Decimal::from_str("2").unwrap()),
                end: Some(Decimal::from_str("3").unwrap()),
                limit: None,
                order_by: Some(1),
            },
        )
        .unwrap();
    assert_eq!(result.ticks.len(), 1);
    assert_eq!(result.ticks[0].price, Decimal::from_str("3").unwrap());
}

#[test]
fn test_market_order() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        ),
        (
            "addr0001",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        ),
        (
            "addr0002",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        ),
        (
            "addr0002",
            &[Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        ),
    ]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    let token_addrs = app
        .set_token_balances(&[(
            "usdt",
            &[
                ("addr0000", 10000000u128),
                ("addr0001", 10000000u128),
                ("addr0002", 10000000u128),
            ],
        )])
        .unwrap();

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

    // Create pair [orai, token_addrs[0]] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::Token {
            contract_addr: token_addrs[0].clone(),
        },
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

    // Submitting a buy market order failed, because any sell orders do not exist

    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(2500000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitMarketOrder {
            direction: OrderDirection::Buy,
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
            ],
            slippage: None,
        })
        .unwrap(),
    };

    let _res = app.execute(
        Addr::unchecked("addr0001"),
        token_addrs[0].clone(),
        &msg,
        &[],
    );
    assert_eq!(_res.is_err(), true);

    // scenario: create 3 sell orders at price 1, 1.1, 1.2. Then, crate a buy order with slippage default is  10%

    let offers: Vec<u128> = vec![1000000, 1000000, 1000000];
    let asks: Vec<u128> = vec![1000000, 1100000, 1200000];
    for i in 0..3 {
        let msg = ExecuteMsg::SubmitOrder {
            direction: OrderDirection::Sell,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(offers[i]),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(asks[i]),
                },
            ],
        };

        let _res = app
            .execute(
                Addr::unchecked("addr0000"),
                orderbook_addr.clone(),
                &msg,
                &[Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(offers[i]),
                }],
            )
            .unwrap();
    }

    // Submitting a buy market order failed, (slippage > 1)
    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(2500000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitMarketOrder {
            direction: OrderDirection::Buy,
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
            ],
            slippage: Some(Decimal::from_str("1.1").unwrap()),
        })
        .unwrap(),
    };

    assert_eq!(
        app.execute(
            Addr::unchecked("addr0001"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .is_err(),
        true
    );

    // current balances: Addr0 (7000000 Orai, 10000000 usdt);  Addr1 (10000000 Orai, 10000000 usdt)

    // create buy order (offer 2500000 usdt, slippage 10%)
    // order 1, order 2 fulfilled,
    // market order: matched 2000000 Orai (offer 2100000 usdt, refund 400000 usdt)
    // addr0 receive:
    //    -  1000000 * 0.999 = 999000 usdt
    //    -  1100000 * 0.999 - 200 * 1.1 = 1098570 usdt
    // total : 998700 + 1098570 = 2097270 usdt
    // addr 1 receive : 2000000 * 0.999 = 1998000 orai
    // balance after:  Addr0 (7000000 Orai, 12097900 usdt);  Addr1 (11997700 Orai, 7900000 usdt)

    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(2500000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitMarketOrder {
            direction: OrderDirection::Buy,
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
            ],
            slippage: None,
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    // current expected balance: Addr0 ()

    let address0_native_balances = app
        .query_balance(Addr::unchecked("addr0000"), ORAI_DENOM.to_string())
        .unwrap();
    let address1_native_balances = app
        .query_balance(Addr::unchecked("addr0001"), ORAI_DENOM.to_string())
        .unwrap();
    let address0_token_balances = app.query_token_balances("addr0000").unwrap();
    let address1_token_balances = app.query_token_balances("addr0001").unwrap();

    assert_eq!(address0_native_balances, Uint128::from(7000000u128));
    assert_eq!(
        address0_token_balances[0],
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(12097900u128),
        },
    );
    assert_eq!(address1_native_balances, Uint128::from(11998000u128));
    assert_eq!(
        address1_token_balances[0],
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(7900000u128),
        },
    );

    // scenario: create  2 others sell orders at price 1.2 , 1.3 (Including the sell order in 1.2 above, there are 3 orders) . Then, crate a buy order with slippage 50%.
    //All sell orders will be matched,

    let offers: Vec<u128> = vec![1000000, 1000000];
    let asks: Vec<u128> = vec![1200000, 1300000];
    for i in 0..2 {
        let msg = ExecuteMsg::SubmitOrder {
            direction: OrderDirection::Sell,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(offers[i]),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(asks[i]),
                },
            ],
        };

        let _res = app
            .execute(
                Addr::unchecked("addr0000"),
                orderbook_addr.clone(),
                &msg,
                &[Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(offers[i]),
                }],
            )
            .unwrap();
    }

    // current balances: Addr0 (5000000 Orai, 12097900 usdt);  Addr1 (11997700 Orai, 7900000 usdt)

    // create buy order (offer 3000000 usdt, slippage 50%)
    // sell order at price 1.2, 1.3 fulfilled, at price 1.4 partial filled
    // market buy order: matched 3000000 usdt
    // addr0 receive:
    //    -  1200000 * 0.999 = 1198800 usdt
    //    -  1200000 * 0.999  = 1198800 usdt
    //    -  600000 * 0.999  = 599400 usdt
    // total : 1198440 + 1198440 + 599010 = 2995890 usdt
    // addr 1 receive : 2000000 * 0.999 + 600000 / 1.3 * 0.999 = 2459076 orai
    // balance after:  Addr0 (5000000 Orai, 15094900 usdt);  Addr1 (14457077 Orai, 4900000 usdt)

    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(3000000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitMarketOrder {
            direction: OrderDirection::Buy,
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
            ],
            slippage: Some(Decimal::from_ratio(50u128, 100u128)),
        })
        .unwrap(),
    };

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            token_addrs[0].clone(),
            &msg,
            &[],
        )
        .unwrap();

    let address0_native_balances = app
        .query_balance(Addr::unchecked("addr0000"), ORAI_DENOM.to_string())
        .unwrap();
    let address1_native_balances = app
        .query_balance(Addr::unchecked("addr0001"), ORAI_DENOM.to_string())
        .unwrap();
    let address0_token_balances = app.query_token_balances("addr0000").unwrap();
    let address1_token_balances = app.query_token_balances("addr0001").unwrap();

    assert_eq!(address0_native_balances, Uint128::from(5000000u128));
    assert_eq!(
        address0_token_balances[0],
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(15094900u128),
        },
    );
    assert_eq!(address1_native_balances, Uint128::from(14457077u128));
    assert_eq!(
        address1_token_balances[0],
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(4900000u128),
        },
    );

    // scenario: create  3 buy orders at price 1.0 1.1 1.2 Then, crate a buy order with slippage 50%.
    // All buy orders will be matched,

    let offers: Vec<u128> = vec![1000000, 1100000, 1200000];
    let asks: Vec<u128> = vec![1000000, 1000000, 1000000];
    for i in 0..3 {
        let msg = cw20::Cw20ExecuteMsg::Send {
            contract: orderbook_addr.to_string(),
            amount: Uint128::new(offers[i]),
            msg: to_json_binary(&Cw20HookMsg::SubmitOrder {
                direction: OrderDirection::Buy,
                assets: [
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: ORAI_DENOM.to_string(),
                        },
                        amount: Uint128::from(asks[i]),
                    },
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: token_addrs[0].clone(),
                        },
                        amount: Uint128::from(offers[i]),
                    },
                ],
            })
            .unwrap(),
        };

        let _res = app
            .execute(
                Addr::unchecked("addr0000"),
                token_addrs[0].clone(),
                &msg,
                &[],
            )
            .unwrap();
    }

    // current balances: Addr0 (5000000 Orai, 11793160 usdt);  Addr1 (14456477 Orai, 4900000 usdt)

    // create market sell order (offer 2500000 orai, slippage 50%)
    // buy order at price 1, 1.1, 1.2 fulfilled
    // market sell order: matched 2500000 orai
    // addr0 receive: 2500000 * 0.999 = 2497500 orai
    // addr 1 receive : 1200000 * 0.999 + 1100000 * 0.999 + 500000 * 1 * 0.999 = 2797200 usdt
    // balance after:  Addr0 (7497500 Orai, 11794900 usdt);  Addr1 (11957077 Orai, 7697200 usdt)

    let msg = ExecuteMsg::SubmitMarketOrder {
        direction: OrderDirection::Sell,
        asset_infos: [
            AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
            AssetInfo::Token {
                contract_addr: token_addrs[0].clone(),
            },
        ],
        slippage: Some(Decimal::from_ratio(50u128, 100u128)),
    };

    // Submitting a buy market order failed, (ProvidesAsset if invalid)
    assert_eq!(
        app.execute(
            Addr::unchecked("addr0002"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(2500000u128),
            }],
        )
        .is_err(),
        true
    );

    let _res = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(2500000u128),
            }],
        )
        .unwrap();

    // current expected balance: Addr0 ()

    let address0_native_balances = app
        .query_balance(Addr::unchecked("addr0000"), ORAI_DENOM.to_string())
        .unwrap();
    let address1_native_balances = app
        .query_balance(Addr::unchecked("addr0001"), ORAI_DENOM.to_string())
        .unwrap();
    let address0_token_balances = app.query_token_balances("addr0000").unwrap();
    let address1_token_balances = app.query_token_balances("addr0001").unwrap();

    assert_eq!(address0_native_balances, Uint128::from(7497500u128));
    assert_eq!(
        address0_token_balances[0],
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(11794900u128),
        },
    );
    assert_eq!(address1_native_balances, Uint128::from(11957077u128));
    assert_eq!(
        address1_token_balances[0],
        Coin {
            denom: USDT_DENOM.to_string(),
            amount: Uint128::from(7697200u128),
        },
    );

    // case submit cw20 market order failed, invalid funds
    let new_tokens = app
        .set_token_balances(&[(
            "uusd",
            &[
                ("addr0000", 10000000u128),
                ("addr0001", 10000000u128),
                ("addr0002", 10000000u128),
            ],
        )])
        .unwrap();

    let msg = cw20::Cw20ExecuteMsg::Send {
        contract: orderbook_addr.to_string(),
        amount: Uint128::new(3000000u128),
        msg: to_json_binary(&Cw20HookMsg::SubmitMarketOrder {
            direction: OrderDirection::Buy,
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                AssetInfo::Token {
                    contract_addr: token_addrs[0].clone(),
                },
            ],
            slippage: Some(Decimal::from_ratio(50u128, 100u128)),
        })
        .unwrap(),
    };

    assert_eq!(
        app.execute(
            Addr::unchecked("addr0001"),
            new_tokens[0].clone(),
            &msg,
            &[],
        )
        .is_err(),
        true
    )
}

#[test]
fn test_query_simulate_market_order() {
    let mut app = MockApp::new(&[
        (
            "addr0000",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        ),
        (
            "addr0001",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        ),
        (
            "addr0002",
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        ),
        (
            "addr0002",
            &[Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        ),
    ]);

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    let token_addrs = app
        .set_token_balances(&[(
            "usdt",
            &[
                ("addr0000", 10000000u128),
                ("addr0001", 10000000u128),
                ("addr0002", 10000000u128),
            ],
        )])
        .unwrap();

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

    // Create pair [orai, token_addrs[0]] for order book
    let msg = ExecuteMsg::CreateOrderBookPair {
        base_coin_info: AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        quote_coin_info: AssetInfo::Token {
            contract_addr: token_addrs[0].clone(),
        },
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

    // Simulate market price with sell orders do not exist

    let res = app
        .query::<SimulateMarketOrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::SimulateMarketOrder {
                direction: OrderDirection::Buy,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                ],
                slippage: None,
                offer_amount: Uint128::from(10000000u128),
            },
        )
        .unwrap_err();
    assert!(res.to_string().contains("Cannot find a matched price"));

    // scenario: create 3 sell orders at price 1, 1.1, 1.2. Then, crate a buy order with slippage default is  10%

    let offers: Vec<u128> = vec![1000000, 1000000, 1000000];
    let asks: Vec<u128> = vec![1000000, 1100000, 1200000];
    for i in 0..3 {
        let msg = ExecuteMsg::SubmitOrder {
            direction: OrderDirection::Sell,
            assets: [
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    amount: Uint128::from(offers[i]),
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                    amount: Uint128::from(asks[i]),
                },
            ],
        };

        let _res = app
            .execute(
                Addr::unchecked("addr0000"),
                orderbook_addr.clone(),
                &msg,
                &[Coin {
                    denom: ORAI_DENOM.to_string(),
                    amount: Uint128::from(offers[i]),
                }],
            )
            .unwrap();
    }

    // Submitting a buy market order with slippage default 0.1 => match sell order at price 1 & 1.1
    // => receive 2000000, with offer 2100000, refund 400000
    let res = app
        .query::<SimulateMarketOrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::SimulateMarketOrder {
                direction: OrderDirection::Buy,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::Token {
                        contract_addr: token_addrs[0].clone(),
                    },
                ],
                slippage: None,
                offer_amount: Uint128::new(2500000u128),
            },
        )
        .unwrap();
    assert_eq!(
        res,
        SimulateMarketOrderResponse {
            receive: Uint128::from(2000000u128),
            refunds: Uint128::from(400000u128)
        }
    );
}

#[test]
fn test_submit_order_with_refunds_offer_asset() {
    let (mut app, orderbook_addr) = basic_fixture();

    // current balance:
    // addr0000: 1000000000 ORAI, 1000000000 USDT
    // addr0001: 1000000000 ORAI, 1000000000 USDT

    // Order 1
    // addr0000 submit buy order at price 2
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(2000000u128),
            },
        ],
    };

    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(2000000u128),
            }],
        )
        .unwrap();

    // current balance:
    // addr0000: 1000000000 ORAI, 998000000 USDT
    // addr0001: 1000000000 ORAI, 1000000000 USDT

    // addr0001 submit a sell order at price 1 (offer 1000000 ORAI, ASK 1000000 USDT)
    // but the highest price of buy order in contract is 2, so user only needs 500000 orai to receive 1000000 usdt => refund 500000 orai
    // balances after:
    // addr0000: 1000000000  + (500000 * 0.999) = 1000499500 ORAI, 998000000 USDT
    // addr0001: 999500000 ORAI, 1000000000 + (1000000 * 0.999 ) =  1000999000 USDT

    // Order 2
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
        ],
    };

    let _ = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000000u128),
            }],
        )
        .unwrap();

    let addr0_native_balance = app.query_all_balances(Addr::unchecked("addr0000")).unwrap();
    let addr1_native_balance = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    assert_eq!(
        addr0_native_balance,
        vec![
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(1000499500u128),
            },
            Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(998000000u128),
            }
        ]
    );
    assert_eq!(
        addr1_native_balance,
        vec![
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(999500000u128),
            },
            Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1000999000u128),
            }
        ]
    );

    // case 2: refunds order has status PartialFilled before being matched

    // addr0 create a buy order at price 10
    // Order 3
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(1000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(10000000u128),
            },
        ],
    };
    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        )
        .unwrap();

    // addr1 create a sell order at price 8, but the order status after matching is PartialFilled
    // balance before: 999500000 ORAI, 1000998700 USDT
    // balance after submit Sell order:989500000 ORAI, 1000999000 + 10000000 * 0.999 = 1010989000 USDT

    // Order 4
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Sell,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(80000000u128),
            },
        ],
    };
    let _ = app
        .execute(
            Addr::unchecked("addr0001"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(10000000u128),
            }],
        )
        .unwrap();
    let addr1_native_balance = app.query_all_balances(Addr::unchecked("addr0001")).unwrap();
    assert_eq!(
        addr1_native_balance,
        vec![
            Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(989500000u128),
            },
            Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(1010989000u128),
            }
        ]
    );
    let order_4 = OrderResponse {
        order_id: 4u64,
        bidder_addr: "addr0001".to_string(),
        offer_asset: Asset {
            amount: Uint128::from(10000000u128),
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
        },
        ask_asset: Asset {
            amount: Uint128::from(80000000u128),
            info: AssetInfo::NativeToken {
                denom: USDT_DENOM.to_string(),
            },
        },
        filled_offer_amount: Uint128::from(1000000u128),
        filled_ask_amount: Uint128::from(10000000u128),
        direction: OrderDirection::Sell,
        status: OrderStatus::PartialFilled,
    };

    assert_eq!(
        order_4,
        app.query::<OrderResponse, _>(
            orderbook_addr.clone(),
            &QueryMsg::Order {
                order_id: 4,
                asset_infos: [
                    AssetInfo::NativeToken {
                        denom: ORAI_DENOM.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: USDT_DENOM.to_string(),
                    },
                ],
            }
        )
        .unwrap()
    );
    // order 5, submit other buy order at price 10, order 4 being fulfilled, and refunds 9000000 - 70000000/8 = 250000 ORAI
    // addr1 balance after: 989500000 + 250000 = 989750000 ORAI
    let msg = ExecuteMsg::SubmitOrder {
        direction: OrderDirection::Buy,
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: Uint128::from(10000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: USDT_DENOM.to_string(),
                },
                amount: Uint128::from(100000000u128),
            },
        ],
    };
    let _ = app
        .execute(
            Addr::unchecked("addr0000"),
            orderbook_addr.clone(),
            &msg,
            &[Coin {
                denom: USDT_DENOM.to_string(),
                amount: Uint128::from(100000000u128),
            }],
        )
        .unwrap();

    let addr1_native_balance = app
        .query_balance(Addr::unchecked("addr0001"), ORAI_DENOM.to_string())
        .unwrap();
    assert_eq!(addr1_native_balance, Uint128::from(989750000u128));
}
