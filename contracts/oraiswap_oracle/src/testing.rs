use cosmwasm_std::testing::MOCK_CONTRACT_ADDR;
use cosmwasm_std::{to_json_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal, Uint128, WasmMsg};

use oraiswap::asset::{Asset, AssetInfo, ORAI_DENOM};
use oraiswap::create_entry_points_testing;
use oraiswap::oracle::{ExecuteMsg, OracleContract};
use oraiswap::testing::{MockApp, APP_OWNER};

fn setup_contract() -> MockApp {
    let mut app = MockApp::new(&[(
        &APP_OWNER.to_string(),
        &[Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(100000u128),
        }],
    )]);

    app.set_oracle_contract(Box::new(create_entry_points_testing!(crate)));

    app
}

#[test]
fn proper_initialization() {
    let mut app = setup_contract();

    let msg = ExecuteMsg::UpdateExchangeRate {
        denom: "usdt".to_string(),
        exchange_rate: Decimal::percent(10), // 1 orai = 10 usdt
    };

    let oracle_contract = OracleContract(app.oracle_addr.clone());
    let contract_addr = app.oracle_addr.clone();
    let _res = app
        .execute(Addr::unchecked(APP_OWNER), contract_addr, &msg, &[])
        .unwrap();

    let exchange_rate_res = oracle_contract
        .query_exchange_rate(
            &app.as_querier().into_empty(),
            "usdt".to_string(),
            ORAI_DENOM.to_string(),
        )
        .unwrap();

    assert_eq!("10", exchange_rate_res.item.exchange_rate.to_string());

    let msg = ExecuteMsg::UpdateExchangeRate {
        denom: "airi".to_string(),
        exchange_rate: Decimal::percent(1), // 1 orai = 100 airi
    };

    let contract_addr = app.oracle_addr.clone();
    let _res = app
        .execute(Addr::unchecked(APP_OWNER), contract_addr, &msg, &[])
        .unwrap();

    let exchange_rate_res = oracle_contract
        .query_exchange_rate(
            &app.as_querier().into_empty(),
            "airi".to_string(),
            "usdt".to_string(),
        )
        .unwrap();

    // 1 usdt = 10 airi
    assert_eq!("10", exchange_rate_res.item.exchange_rate.to_string());
}

#[test]
fn tax_cap_notfound() {
    let app = setup_contract();

    let oracle_contract = OracleContract(app.oracle_addr.clone());

    let res = oracle_contract.query_tax_cap(&app.as_querier().into_empty(), "airi".to_string());
    println!("{:?}", res);
    match res {
        Err(err) => {
            assert_eq!(
                err.to_string()
                    .contains("Tax cap not found for denom: airi"),
                true
            )
        }
        _ => panic!("DO NOT ENTER HERE"),
    }
}

#[test]
fn test_asset() {
    let mut app = setup_contract();

    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));

    app.set_token_balances(&[(
        "asset",
        &[
            (&MOCK_CONTRACT_ADDR.to_string(), 123u128),
            ("addr00000", 123u128),
            ("addr00001", 123u128),
            ("addr00002", 123u128),
        ],
    )])
    .unwrap();

    // set code implementation
    app.set_oracle_contract(Box::new(create_entry_points_testing!(crate)));

    app.set_tax(Decimal::percent(1), &[("uusd", 1000000u128)]);

    let token_asset = Asset {
        amount: Uint128::from(123123u128),
        info: AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0000"),
        },
    };

    let native_token_asset = Asset {
        amount: Uint128::from(123123u128),
        info: AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
    };

    let orai_oracle = OracleContract(app.oracle_addr.clone());

    assert_eq!(
        token_asset
            .compute_tax(&orai_oracle, &app.as_querier().into_empty())
            .unwrap(),
        Uint128::zero()
    );
    assert_eq!(
        native_token_asset
            .compute_tax(&orai_oracle, &app.as_querier().into_empty())
            .unwrap(),
        Uint128::from(1220u128)
    );

    assert_eq!(
        native_token_asset
            .amount
            .checked_sub(
                native_token_asset
                    .compute_tax(&orai_oracle, &app.as_querier().into_empty())
                    .unwrap()
            )
            .unwrap(),
        Uint128::from(121903u128)
    );

    assert_eq!(
        token_asset
            .into_msg(
                Some(&orai_oracle),
                &app.as_querier().into_empty(),
                Addr::unchecked("addr0000")
            )
            .unwrap(),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "asset0000".into(),
            msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
                recipient: "addr0000".into(),
                amount: Uint128::from(123123u128),
            })
            .unwrap(),
            funds: vec![],
        })
    );

    assert_eq!(
        native_token_asset
            .into_msg(
                Some(&orai_oracle),
                &app.as_querier().into_empty(),
                Addr::unchecked("addr0000")
            )
            .unwrap(),
        CosmosMsg::Bank(BankMsg::Send {
            to_address: "addr0000".into(),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(121903u128),
            }]
        })
    );
}
