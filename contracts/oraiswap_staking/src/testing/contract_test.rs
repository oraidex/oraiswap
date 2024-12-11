use crate::contract::{execute, instantiate, query};
use cosmwasm_std::testing::{
    mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
};
use cosmwasm_std::{
    attr, coin, from_json, to_json_binary, Addr, Decimal, Order, StdError, Uint128,
};
use cw20::Cw20ReceiveMsg;
use oraiswap::asset::{Asset, AssetInfo, ORAI_DENOM};
use oraiswap::staking::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, PoolInfoResponse, QueryMsg,
    RewardInfoResponse,
};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        owner: Some(Addr::unchecked("owner")),
        rewarder: Addr::unchecked("reward"),
        minter: Some(Addr::unchecked("mint")),
        oracle_addr: Addr::unchecked("oracle"),
        factory_addr: Addr::unchecked("factory"),
        base_denom: None,
    };

    let info = mock_info("addr", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // it worked, let's query the state
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_json(&res).unwrap();
    assert_eq!(
        ConfigResponse {
            owner: Addr::unchecked("owner"),
            rewarder: Addr::unchecked("reward"),
            oracle_addr: Addr::unchecked("oracle"),
            factory_addr: Addr::unchecked("factory"),
            base_denom: ORAI_DENOM.to_string(),
        },
        config
    );
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        owner: Some(Addr::unchecked("owner")),
        rewarder: Addr::unchecked("reward"),
        minter: Some(Addr::unchecked("mint")),
        oracle_addr: Addr::unchecked("oracle"),
        factory_addr: Addr::unchecked("factory"),
        base_denom: None,
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // update owner
    let info = mock_info("owner", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        owner: Some(Addr::unchecked("owner2")),
        rewarder: None,
        migrate_store_status: Some(true),
    };

    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_json(&res).unwrap();
    assert_eq!(
        ConfigResponse {
            owner: Addr::unchecked("owner2"),
            rewarder: Addr::unchecked("reward"),
            oracle_addr: Addr::unchecked("oracle"),
            factory_addr: Addr::unchecked("factory"),
            base_denom: ORAI_DENOM.to_string(),
        },
        config
    );

    // unauthorized err
    let info = mock_info("owner", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        rewarder: None,
        owner: None,
        migrate_store_status: None,
    };

    let res = execute(deps.as_mut(), mock_env(), info, msg);
    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "unauthorized"),
        _ => panic!("Must return unauthorized error"),
    }
}

#[test]
fn test_register() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        owner: Some(Addr::unchecked("owner")),
        rewarder: Addr::unchecked("reward"),
        minter: Some(Addr::unchecked("mint")),
        oracle_addr: Addr::unchecked("oracle"),
        factory_addr: Addr::unchecked("factory"),
        base_denom: None,
    };

    let info = mock_info("addr", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::RegisterAsset {
        staking_token: Addr::unchecked("staking"),
    };

    // failed with unauthorized error
    let info = mock_info("addr", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap_err();
    match res {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "unauthorized"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    let info = mock_info("owner", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "register_asset"),
            attr("staking_token", "staking"),
        ]
    );

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::PoolInfo {
            staking_token: Addr::unchecked("staking"),
        },
    )
    .unwrap();
    let pool_info: PoolInfoResponse = from_json(&res).unwrap();
    assert_eq!(
        pool_info,
        PoolInfoResponse {
            staking_token: Addr::unchecked("staking"),
            total_bond_amount: Uint128::zero(),
            reward_index: Decimal::zero(),
            pending_reward: Uint128::zero(),
            migration_deprecated_staking_token: None,
            migration_index_snapshot: None,
        }
    );
}

#[test]
fn test_query_staker_pagination() {
    let mut deps = mock_dependencies_with_balance(&[coin(10000000000u128, ORAI_DENOM)]);

    let msg = InstantiateMsg {
        owner: Some(Addr::unchecked("owner")),
        rewarder: Addr::unchecked("reward"),
        minter: Some(Addr::unchecked("mint")),
        oracle_addr: Addr::unchecked("oracle"),
        factory_addr: Addr::unchecked("factory"),
        base_denom: None,
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // set rewards per second for asset
    // will also add to the index the pending rewards from before the migration
    let msg = ExecuteMsg::UpdateRewardsPerSec {
        staking_token: Addr::unchecked("staking"),
        assets: vec![Asset {
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
            amount: 100u128.into(),
        }],
    };
    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::RegisterAsset {
        staking_token: Addr::unchecked("staking"),
    };

    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // bond 100 tokens for 100 stakers
    for i in 0..100 {
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: format!("addr{}", i),
            amount: Uint128::from(100u128),
            msg: to_json_binary(&Cw20HookMsg::Bond {}).unwrap(),
        });
        let info = mock_info("staking", &[]);
        let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    }

    // query stakers by order
    let mut start_after: Option<Addr> = None;
    for _ in 0..100 / 10 {
        let data = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::RewardInfos {
                staking_token: Addr::unchecked("staking"),
                limit: Some(10),
                order: Some(Order::Ascending.into()),
                start_after: start_after.clone(),
            },
        )
        .unwrap();
        let res: Vec<RewardInfoResponse> = from_json(&data).unwrap();
        let stakers: Vec<Addr> = res.into_iter().map(|r| r.staker_addr).collect();
        let staker_addrs: Vec<String> =
            stakers.clone().into_iter().map(|s| s.to_string()).collect();
        start_after = stakers.into_iter().last();
        println!("{:?}", staker_addrs);
    }
}
