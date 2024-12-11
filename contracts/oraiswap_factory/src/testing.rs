use cosmwasm_std::Addr;
use oraiswap::asset::{AssetInfo, PairInfo};

use oraiswap::create_entry_points_testing;
use oraiswap::pair::{DEFAULT_COMMISSION_RATE, DEFAULT_OPERATOR_FEE};
use oraiswap::querier::query_pair_info_from_pair;
use oraiswap::testing::MockApp;

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

    // create pair
    let contract_addr = app.create_pair(asset_infos.clone()).unwrap();

    // query pair info
    let pair_info =
        query_pair_info_from_pair(&app.as_querier().into_empty(), contract_addr.clone()).unwrap();

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
