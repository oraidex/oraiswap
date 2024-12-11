use crate::contract::{execute, instantiate, query, query_get_pools_infomation};
use crate::state::{store_pool_info, PoolInfo};
use cosmwasm_std::testing::{
    mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
};
use cosmwasm_std::{
    attr, coin, from_json, to_json_binary, Addr, Api, BankMsg, Coin, CosmosMsg, Decimal, StdError,
    SubMsg, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use oraiswap::asset::{Asset, AssetInfo, ORAI_DENOM};
use oraiswap::create_entry_points_testing;
use oraiswap::pair::PairResponse;
use oraiswap::staking::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, PoolInfoResponse, QueryMsg, RewardInfoResponse,
    RewardInfoResponseItem, RewardMsg,
};
use oraiswap::testing::{AttributeUtil, MockApp, ATOM_DENOM};

#[test]
fn test_query_all_pool_keys() {
    let mut deps = mock_dependencies();
    let first_staking_token = Addr::unchecked("staking1");
    let second_staking_token = Addr::unchecked("staking2");
    let first_staking_canon = deps
        .api
        .addr_canonicalize(first_staking_token.as_str())
        .unwrap();
    let second_staking_canon = deps
        .api
        .addr_canonicalize(second_staking_token.as_str())
        .unwrap();

    let deps_mut = deps.as_mut();
    let storage = deps_mut.storage;

    // populate fake data, can change to 100 if want
    for n in 0..10u64 {
        let amount = Uint128::from(n);
        let (asset_key, staking_token) = if n < 5 {
            (first_staking_canon.clone(), first_staking_canon.clone())
        } else {
            (second_staking_canon.clone(), second_staking_canon.clone())
        };
        let pool_info = PoolInfo {
            staking_token: staking_token.clone(),
            pending_reward: amount.clone(),
            total_bond_amount: amount.clone(),
            reward_index: Decimal::zero(),
            migration_params: None,
        };
        store_pool_info(storage, &asset_key, &pool_info).unwrap();
    }

    let all_pool_keys = query_get_pools_infomation(deps.as_ref()).unwrap();
    assert_eq!(all_pool_keys.len(), 2);
    // assert_eq!(
    //     all_pool_keys.contains(&first_staking_token.to_string()),
    //     true
    // );
    // assert_eq!(
    //     all_pool_keys.contains(&second_staking_token.to_string()),
    //     true
    // );
}

#[test]
fn test_bond_tokens() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        owner: Some(Addr::unchecked("owner")),
        rewarder: Addr::unchecked("rewarder"),
        minter: Some(Addr::unchecked("mint")),
        oracle_addr: Addr::unchecked("oracle"),
        factory_addr: Addr::unchecked("factory"),
        base_denom: None,
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::RegisterAsset {
        staking_token: Addr::unchecked("staking"),
    };

    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr".to_string(),
        amount: Uint128::from(100u128),
        msg: to_json_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });

    let info = mock_info("staking", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    let data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::RewardInfo {
            staking_token: Some(Addr::unchecked("staking")),
            staker_addr: Addr::unchecked("addr"),
        },
    )
    .unwrap();
    let res: RewardInfoResponse = from_json(&data).unwrap();
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: Addr::unchecked("addr"),
            reward_infos: vec![RewardInfoResponseItem {
                staking_token: Addr::unchecked("staking"),
                pending_reward: Uint128::zero(),
                pending_withdraw: vec![],
                bond_amount: Uint128::from(100u128),
                should_migrate: None,
            }],
        }
    );

    let data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::PoolInfo {
            staking_token: Addr::unchecked("staking"),
        },
    )
    .unwrap();

    let pool_info: PoolInfoResponse = from_json(&data).unwrap();
    assert_eq!(
        pool_info,
        PoolInfoResponse {
            staking_token: Addr::unchecked("staking"),
            total_bond_amount: Uint128::from(100u128),
            reward_index: Decimal::zero(),
            pending_reward: Uint128::zero(),
            migration_deprecated_staking_token: None,
            migration_index_snapshot: None,
        }
    );

    // bond 100 more tokens from other account
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr2".to_string(),
        amount: Uint128::from(100u128),
        msg: to_json_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    let info = mock_info("staking", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    let data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::PoolInfo {
            staking_token: Addr::unchecked("staking"),
        },
    )
    .unwrap();
    let pool_info: PoolInfoResponse = from_json(&data).unwrap();
    assert_eq!(
        pool_info,
        PoolInfoResponse {
            staking_token: Addr::unchecked("staking"),
            total_bond_amount: Uint128::from(200u128),
            reward_index: Decimal::zero(),
            pending_reward: Uint128::zero(),
            migration_deprecated_staking_token: None,
            migration_index_snapshot: None,
        }
    );
}

#[test]
fn test_unbond() {
    let mut deps = mock_dependencies_with_balance(&[
        coin(10000000000u128, ORAI_DENOM),
        coin(20000000000u128, ATOM_DENOM),
    ]);

    let msg = InstantiateMsg {
        owner: Some(Addr::unchecked("owner")),
        rewarder: Addr::unchecked("rewarder"),
        minter: Some(Addr::unchecked("mint")),
        oracle_addr: Addr::unchecked("oracle"),
        factory_addr: Addr::unchecked("factory"),
        base_denom: None,
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // will also add to the index the pending rewards from before the migration
    let msg = ExecuteMsg::UpdateRewardsPerSec {
        staking_token: Addr::unchecked("staking"),
        assets: vec![
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: 100u128.into(),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: 200u128.into(),
            },
        ],
    };
    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // register asset
    let msg = ExecuteMsg::RegisterAsset {
        staking_token: Addr::unchecked("staking"),
    };

    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // bond 100 tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr".to_string(),
        amount: Uint128::from(100u128),
        msg: to_json_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    let info = mock_info("staking", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::DepositReward {
        rewards: vec![RewardMsg {
            staking_token: Addr::unchecked("staking"),
            total_accumulation_amount: Uint128::from(300u128),
        }],
    };
    let info = mock_info("rewarder", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

    // will also add to the index the pending rewards from before the migration
    let msg = ExecuteMsg::UpdateRewardsPerSec {
        staking_token: Addr::unchecked("staking"),
        assets: vec![
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
                },
                amount: 100u128.into(),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ATOM_DENOM.to_string(),
                },
                amount: 100u128.into(),
            },
        ],
    };
    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // unbond 150 tokens; failed
    let msg = ExecuteMsg::Unbond {
        staking_token: Addr::unchecked("staking"),
        amount: Uint128::from(150u128),
    };

    let info = mock_info("addr", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
    match res {
        StdError::GenericErr { msg, .. } => {
            assert_eq!(msg, "Cannot unbond more than bond amount");
        }
        _ => panic!("Must return generic error"),
    };

    // normal unbond
    let msg = ExecuteMsg::Unbond {
        staking_token: Addr::unchecked("staking"),
        amount: Uint128::from(100u128),
    };

    let info = mock_info("addr", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "staking".to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "addr".to_string(),
                    amount: Uint128::from(100u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "addr".to_string(),
                amount: vec![coin(99u128, ORAI_DENOM)],
            })),
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "addr".to_string(),
                amount: vec![coin(199u128, ATOM_DENOM)],
            }))
        ]
    );

    let data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::PoolInfo {
            staking_token: Addr::unchecked("staking"),
        },
    )
    .unwrap();
    let pool_info: PoolInfoResponse = from_json(&data).unwrap();
    assert_eq!(
        pool_info,
        PoolInfoResponse {
            staking_token: Addr::unchecked("staking"),
            total_bond_amount: Uint128::zero(),
            reward_index: Decimal::from_ratio(300u128, 100u128),
            pending_reward: Uint128::zero(),
            migration_deprecated_staking_token: None,
            migration_index_snapshot: None,
        }
    );

    let data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::RewardInfo {
            staking_token: None,
            staker_addr: Addr::unchecked("addr"),
        },
    )
    .unwrap();
    let res: RewardInfoResponse = from_json(&data).unwrap();
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: Addr::unchecked("addr"),
            reward_infos: vec![],
        }
    );
}

#[test]
fn test_auto_stake() {
    let mut app = MockApp::new(&[("addr", &[coin(10000000000u128, ORAI_DENOM)])]);

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

    let asset_addr = app.create_token("asset");
    let reward_addr = app.create_token("reward");

    // update other contract token balance
    app.set_token_balances(&[
        ("reward", &[("addr", 10000000000u128)]),
        ("asset", &[("addr", 10000000000u128)]),
    ])
    .unwrap();

    let asset_infos = [
        AssetInfo::NativeToken {
            denom: ORAI_DENOM.to_string(),
        },
        AssetInfo::Token {
            contract_addr: asset_addr.clone(),
        },
    ];

    // create pair
    let pair_addr = app.create_pair(asset_infos.clone()).unwrap();

    let PairResponse { info: pair_info } = app
        .query(pair_addr.clone(), &oraiswap::pair::QueryMsg::Pair {})
        .unwrap();

    // set allowance
    app.execute(
        Addr::unchecked("addr"),
        asset_addr.clone(),
        &cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: pair_addr.to_string(),
            amount: Uint128::from(100u128),
            expires: None,
        },
        &[],
    )
    .unwrap();

    // provide liquidity
    // successfully provide liquidity for the exist pool
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

    let _res = app
        .execute(
            Addr::unchecked("addr"),
            pair_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(100u128),
            }],
        )
        .unwrap();

    let code_id = app.upload(Box::new(create_entry_points_testing!(crate)));

    let msg = InstantiateMsg {
        owner: Some(Addr::unchecked("owner")),
        rewarder: reward_addr.clone(),
        minter: Some(Addr::unchecked("mint")),
        oracle_addr: app.oracle_addr.clone(),
        factory_addr: app.factory_addr.clone(),
        base_denom: None,
    };

    let staking_addr = app
        .instantiate(code_id, Addr::unchecked("addr"), &msg, &[], "staking")
        .unwrap();

    // set allowance
    app.execute(
        Addr::unchecked("addr"),
        asset_addr.clone(),
        &cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: staking_addr.to_string(),
            amount: Uint128::from(100u128),
            expires: None,
        },
        &[],
    )
    .unwrap();

    let msg = ExecuteMsg::RegisterAsset {
        staking_token: pair_info.liquidity_token.clone(),
    };

    let _res = app
        .execute(Addr::unchecked("owner"), staking_addr.clone(), &msg, &[])
        .unwrap();

    // no token asset
    let msg = ExecuteMsg::AutoStake {
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: ORAI_DENOM.to_string(),
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
    };

    app.execute(
        Addr::unchecked("addr"),
        staking_addr.clone(),
        &msg,
        &[Coin {
            denom: ORAI_DENOM.to_string(),
            amount: Uint128::from(100u128),
        }],
    )
    .unwrap_err();

    // no native asset
    let msg = ExecuteMsg::AutoStake {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: asset_addr.clone(),
                },
                amount: Uint128::from(1u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: asset_addr.clone(),
                },
                amount: Uint128::from(1u128),
            },
        ],
        slippage_tolerance: None,
    };

    app.execute(Addr::unchecked("addr"), staking_addr.clone(), &msg, &[])
        .unwrap_err();

    let msg = ExecuteMsg::AutoStake {
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
                amount: Uint128::from(1u128),
            },
        ],
        slippage_tolerance: None,
    };

    // attempt with no coins
    let error = app
        .execute(Addr::unchecked("addr"), staking_addr.clone(), &msg, &[])
        .unwrap_err();
    assert!(error
        .root_cause()
        .to_string()
        .contains("Native token balance mismatch between the argument and the transferred"));

    let _res = app
        .execute(
            Addr::unchecked("addr"),
            staking_addr.clone(),
            &msg,
            &[Coin {
                denom: ORAI_DENOM.to_string(),
                amount: Uint128::from(100u128),
            }],
        )
        .unwrap();

    // wrong asset
    let msg = ExecuteMsg::AutoStakeHook {
        staking_token: pair_info.liquidity_token.clone(),
        staker_addr: Addr::unchecked("addr"),
        prev_staking_token_amount: Uint128::zero(),
    };
    let _res = app.execute(staking_addr.clone(), staking_addr.clone(), &msg, &[]);

    // valid msg
    let msg = ExecuteMsg::AutoStakeHook {
        staking_token: pair_info.liquidity_token.clone(),
        staker_addr: Addr::unchecked("addr"),
        prev_staking_token_amount: Uint128::zero(),
    };

    // unauthorized attempt
    let error = app
        .execute(Addr::unchecked("addr"), staking_addr.clone(), &msg, &[])
        .unwrap_err();
    assert!(error.root_cause().to_string().contains("unauthorized"));

    // successfull attempt

    let res = app
        .execute(staking_addr.clone(), staking_addr.clone(), &msg, &[])
        .unwrap();
    assert_eq!(
        // first attribute is _contract_addr
        res.get_attributes(1),
        vec![
            attr("action", "bond"),
            attr("staker_addr", "addr"),
            attr("staking_token", pair_info.liquidity_token.as_str()),
            attr("amount", "1"),
        ]
    );

    let pool_info: PoolInfoResponse = app
        .query(
            staking_addr.clone(),
            &QueryMsg::PoolInfo {
                staking_token: pair_info.liquidity_token.clone(),
            },
        )
        .unwrap();

    assert_eq!(
        pool_info,
        PoolInfoResponse {
            staking_token: pair_info.liquidity_token.clone(),
            total_bond_amount: Uint128::from(3u128),
            reward_index: Decimal::zero(),
            pending_reward: Uint128::zero(),
            migration_deprecated_staking_token: None,
            migration_index_snapshot: None,
        }
    );
}
