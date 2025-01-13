use std::str::FromStr;

use cosmwasm_std::{coin, to_json_binary, Addr, Coin};
use oraiswap::asset::{AssetInfo, PairInfo};

use oraiswap::create_entry_points_testing;
use oraiswap::factory::ConfigResponse;
use oraiswap::pair::{PairResponse, DEFAULT_COMMISSION_RATE, DEFAULT_OPERATOR_FEE};
use oraiswap::querier::query_pair_info_from_pair;
use oraiswap::testing::{MockApp, APP_OWNER};

#[test]
fn create_pair() {
    let mut app = MockApp::new(&[]);
    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));
    app.set_oracle_contract(Box::new(create_entry_points_testing!(oraiswap_oracle)));

    app.set_factory_and_pair_contract(
        Box::new(create_entry_points_testing!(crate).with_reply_empty(crate::contract::reply)),
        Box::new(
            create_entry_points_testing!(oraiswap_pair)
                .with_reply_empty(oraiswap_pair::contract::reply),
        ),
    );

    let contract_addr1 = app.create_token("assetC");
    let contract_addr2 = app.create_token("assetD");

    app.mint_token(contract_addr1.clone(), 1000000u128).unwrap();
    app.mint_token(contract_addr2.clone(), 1000000u128).unwrap();
    app.increase_allowance(contract_addr1.clone(), 1000000u128)
        .unwrap();
    app.increase_allowance(contract_addr2.clone(), 1000000u128)
        .unwrap();

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: contract_addr1,
        },
        AssetInfo::Token {
            contract_addr: contract_addr2,
        },
    ];

    // create pair
    let contract_addr = app
        .create_pair_add_add_liquidity(asset_infos.clone())
        .unwrap();

    // query pair info
    let pair_info =
        query_pair_info_from_pair(&app.as_querier().into_empty(), contract_addr.clone()).unwrap();

    // get config
    let config: String = app
        .as_querier()
        .query_wasm_smart(
            contract_addr.clone(),
            &oraiswap::pair::QueryMsg::Operator {},
        )
        .unwrap();

    let factory_config: ConfigResponse = app
        .query(
            app.factory_addr.clone(),
            &oraiswap::factory::QueryMsg::Config {},
        )
        .unwrap();

    assert_eq!(config, factory_config.operator);

    // should never change commission rate once deployed
    let pair_res = app.query_pair(asset_infos.clone()).unwrap();
    assert_eq!(
        pair_res,
        PairInfo {
            oracle_addr: app.oracle_addr,
            liquidity_token: pair_info.liquidity_token,
            contract_addr,
            asset_infos,
            commission_rate: DEFAULT_COMMISSION_RATE.into(),
            operator_fee: DEFAULT_OPERATOR_FEE.to_string()
        }
    );
}

#[test]
fn create_pair_restricted() {
    let denom_1 = "factory/orai1token/token1";
    let denom_2 = "factory/orai1hehe/token2";

    let init_balance: &[(&str, &[Coin])] = &[
        (
            APP_OWNER,
            &[coin(1000000000000000000u128, "factory/orai1token1")],
        ),
        (
            APP_OWNER,
            &[coin(1000000000000000000u128, "factory/orai1token2")],
        ),
    ];
    let mut app = MockApp::new(&init_balance);
    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));
    app.set_oracle_contract(Box::new(create_entry_points_testing!(oraiswap_oracle)));

    app.set_factory_and_pair_contract(
        Box::new(create_entry_points_testing!(crate).with_reply_empty(crate::contract::reply)),
        Box::new(
            create_entry_points_testing!(oraiswap_pair)
                .with_reply_empty(oraiswap_pair::contract::reply),
        ),
    );

    let asset_infos = [
        AssetInfo::NativeToken {
            denom: String::from_str(&denom_1).unwrap(),
        },
        AssetInfo::NativeToken {
            denom: String::from_str(&denom_2).unwrap(),
        },
    ];

    // restrict
    app.restrict_asset("factory/orai1token".to_string())
        .unwrap();
    let restrict_prefix = app.query_restrict_denom().unwrap();
    assert_eq!(restrict_prefix.prefixes.len(), 1);

    // create pair failed
    app.create_pair_by(asset_infos.clone(), "user1".to_string())
        .unwrap_err();

    // add creator
    app.add_creator(APP_OWNER.to_string()).unwrap();
    let creators = app.query_creators().unwrap();
    assert_eq!(creators.creators.len(), 1);
    assert_eq!(creators.creators[0].to_string(), APP_OWNER.to_string());

    let contract_addr = app.create_pair(asset_infos.clone()).unwrap();

    // query pair info
    let pair_info =
        query_pair_info_from_pair(&app.as_querier().into_empty(), contract_addr.clone()).unwrap();

    // get config
    let config: String = app
        .as_querier()
        .query_wasm_smart(
            contract_addr.clone(),
            &oraiswap::pair::QueryMsg::Operator {},
        )
        .unwrap();

    let factory_config: ConfigResponse = app
        .query(
            app.factory_addr.clone(),
            &oraiswap::factory::QueryMsg::Config {},
        )
        .unwrap();

    assert_eq!(config, factory_config.operator);

    // should never change commission rate once deployed
    let pair_res = app.query_pair(asset_infos.clone()).unwrap();
    assert_eq!(
        pair_res,
        PairInfo {
            oracle_addr: app.oracle_addr,
            liquidity_token: pair_info.liquidity_token,
            contract_addr,
            asset_infos,
            commission_rate: DEFAULT_COMMISSION_RATE.into(),
            operator_fee: DEFAULT_OPERATOR_FEE.to_string()
        }
    );
}

#[test]
fn add_pair() {
    let mut app = MockApp::new(&[]);
    app.set_token_contract(Box::new(create_entry_points_testing!(oraiswap_token)));
    app.set_oracle_contract(Box::new(create_entry_points_testing!(oraiswap_oracle)));

    app.set_factory_and_pair_contract(
        Box::new(create_entry_points_testing!(crate).with_reply_empty(crate::contract::reply)),
        Box::new(
            create_entry_points_testing!(oraiswap_pair)
                .with_reply_empty(oraiswap_pair::contract::reply),
        ),
    );

    let contract_addr1 = app.create_token("assetA");
    let contract_addr2 = app.create_token("assetB");

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: contract_addr1,
        },
        AssetInfo::Token {
            contract_addr: contract_addr2,
        },
    ];

    let pair_info = PairInfo {
        oracle_addr: app.oracle_addr.clone(),
        liquidity_token: Addr::unchecked("liquidity_token"),
        contract_addr: Addr::unchecked("contract_addr"),
        asset_infos: asset_infos.clone(),
        commission_rate: DEFAULT_COMMISSION_RATE.into(),
        operator_fee: DEFAULT_OPERATOR_FEE.to_string(),
    };

    // add pair
    app.add_pair(pair_info.clone()).unwrap();

    let pair_res = app.query_pair(asset_infos.clone()).unwrap();
    assert_eq!(pair_res, pair_info);
}
