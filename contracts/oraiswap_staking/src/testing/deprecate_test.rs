use crate::contract::{execute, instantiate, query};
use crate::state::{read_pool_info, store_pool_info};
use cosmwasm_std::testing::{mock_dependencies_with_balance, mock_env, mock_info};
use cosmwasm_std::{coin, from_json, to_json_binary, Addr, Api, Decimal, SubMsg, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use oraiswap::asset::{Asset, AssetInfo, ORAI_DENOM};
use oraiswap::staking::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, PoolInfoResponse, QueryMsg, RewardInfoResponse,
    RewardInfoResponseItem, RewardMsg,
};
use oraiswap::testing::ATOM_DENOM;

#[test]
fn test_deprecate() {
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

    let msg = ExecuteMsg::RegisterAsset {
        staking_token: Addr::unchecked("staking"),
    };

    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    let asset_key = deps.api.addr_canonicalize("staking").unwrap();
    let pool_info = read_pool_info(&deps.storage, &asset_key).unwrap();
    store_pool_info(&mut deps.storage, &asset_key, &pool_info).unwrap();

    // set rewards per second for asset
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

    // bond 100 tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr".to_string(),
        amount: Uint128::from(100u128),
        msg: to_json_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    let info = mock_info("staking", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // owner of reward contract deposit 100 reward tokens
    // distribute weight => 80:20
    let msg = ExecuteMsg::DepositReward {
        rewards: vec![RewardMsg {
            staking_token: Addr::unchecked("staking"),
            total_accumulation_amount: Uint128::from(100u128),
        }],
    };
    let info = mock_info("rewarder", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // query pool and reward info
    let res: PoolInfoResponse = from_json(
        &query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::PoolInfo {
                staking_token: Addr::unchecked("staking"),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let res_cmp = res.clone();
    assert_eq!(
        res_cmp,
        PoolInfoResponse {
            total_bond_amount: Uint128::from(100u128),
            reward_index: Decimal::from_ratio(100u128, 100u128),
            migration_index_snapshot: None,
            migration_deprecated_staking_token: None,
            ..res
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
            reward_infos: vec![RewardInfoResponseItem {
                staking_token: Addr::unchecked("staking"),
                bond_amount: Uint128::from(100u128),
                pending_reward: Uint128::from(100u128),
                pending_withdraw: vec![],
                should_migrate: None,
            }],
        }
    );

    // execute deprecate
    let msg = ExecuteMsg::DeprecateStakingToken {
        staking_token: Addr::unchecked("staking"),
        new_staking_token: Addr::unchecked("new_staking"),
    };
    let info = mock_info("owner", &[]);
    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // deposit more rewards
    let msg = ExecuteMsg::DepositReward {
        rewards: vec![RewardMsg {
            staking_token: Addr::unchecked("staking"),
            total_accumulation_amount: Uint128::from(100u128),
        }],
    };
    let info = mock_info("rewarder", &[]);
    // not found
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();

    // query again
    let res: PoolInfoResponse = from_json(
        &query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::PoolInfo {
                staking_token: Addr::unchecked("new_staking"),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let res_cmp = res.clone();
    assert_eq!(
        res_cmp,
        PoolInfoResponse {
            staking_token: Addr::unchecked("new_staking"),
            total_bond_amount: Uint128::from(100u128), // reset
            reward_index: Decimal::from_ratio(100u128, 100u128), // stays the same
            migration_index_snapshot: Some(Decimal::from_ratio(100u128, 100u128)),
            migration_deprecated_staking_token: Some(Addr::unchecked("staking")),
            pending_reward: Uint128::from(0u128), // new reward waiting here
            ..res
        }
    );
    let data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::RewardInfo {
            staking_token: Some(Addr::unchecked("new_staking")),
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

    // try to bond new or old staking token, should fail both
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr".to_string(),
        amount: Uint128::from(100u128),
        msg: to_json_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    let info = mock_info("staking", &[]);
    let _err = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap_err();
    let info = mock_info("new_staking", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // unbond all the old tokens
    let msg = ExecuteMsg::Unbond {
        staking_token: Addr::unchecked("new_staking"),
        amount: Uint128::from(100u128),
    };
    let info = mock_info("addr", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    // make sure that we are receiving deprecated lp tokens tokens
    assert_eq!(
        res.messages,
        vec![SubMsg::new(WasmMsg::Execute {
            contract_addr: "new_staking".into(),
            msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr".to_string(),
                amount: Uint128::from(100u128),
            })
            .unwrap(),
            funds: vec![],
        })]
    );
    let data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::RewardInfo {
            staking_token: Some(Addr::unchecked("new_staking")),
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

    // now can bond the new staking token
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr".to_string(),
        amount: Uint128::from(100u128),
        msg: to_json_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    let info = mock_info("new_staking", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // deposit new rewards
    // will also add to the index the pending rewards from before the migration
    let msg = ExecuteMsg::DepositReward {
        rewards: vec![RewardMsg {
            staking_token: Addr::unchecked("staking"),
            total_accumulation_amount: Uint128::from(100u128),
        }],
    };
    let info = mock_info("rewarder", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();

    // expect to have 80 * 3 rewards
    // initial + deposit after deprecation + deposit after bonding again
    let _data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::RewardInfo {
            staking_token: None,
            staker_addr: Addr::unchecked("addr"),
        },
    )
    .unwrap_err();

    // completely new users can bond
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "newaddr".into(),
        amount: Uint128::from(100u128),
        msg: to_json_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    let info = mock_info("new_staking", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    let data = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::RewardInfo {
            staking_token: None,
            staker_addr: Addr::unchecked("newaddr"),
        },
    )
    .unwrap();
    let res: RewardInfoResponse = from_json(&data).unwrap();
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: Addr::unchecked("newaddr"),
            reward_infos: vec![RewardInfoResponseItem {
                staking_token: Addr::unchecked("new_staking"),
                bond_amount: Uint128::from(100u128),
                pending_reward: Uint128::zero(),
                pending_withdraw: vec![],
                should_migrate: None,
            },],
        }
    );
}
