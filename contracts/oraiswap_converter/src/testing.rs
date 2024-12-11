use std::str::FromStr;

use cosmwasm_std::{
    attr, coin, from_json,
    testing::{mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info},
    to_json_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal, StdError, SubMsg, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use oraiswap::{
    asset::{AssetInfo, ORAI_DENOM},
    converter::{
        ConvertInfoResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, TokenInfo,
        TokenRatio,
    },
    math::Converter128,
    testing::ATOM_DENOM,
};

use crate::contract::{execute, instantiate, query};

#[test]
fn test_decimal_valid_same_decimal() {
    let result = Uint128::from(1_000_000_000_000_000_000u128)
        .checked_div_decimal(Decimal::from_ratio(10u128.pow(6u32), 10u128.pow(7u32)))
        .unwrap();

    assert_eq!(result, Uint128::from(10_000_000_000_000_000_000u128));

    println!(
        "{}",
        Uint128::from(3u128)
            .checked_div_decimal(Decimal::from_str("0.3").unwrap())
            .unwrap()
    );
}

#[test]
fn test_decimal_valid_different_decimal() {
    let result = Uint128::from(1u128)
        .checked_div_decimal(Decimal::from_ratio(10u128.pow(6u32), 10u128.pow(18u32)))
        .unwrap();

    assert_eq!(result, Uint128::from(1000000000000u128))
}

#[test]
fn test_decimal_valid_large_number() {
    let result = Uint128::from(100000000000000000000000000000000000000u128)
        .checked_div_decimal(Decimal::from_ratio(10u128.pow(18u32), 10u128.pow(6u32)));

    println!("result: {:?}", result)
}

#[test]
fn test_convert_reverse() {
    let mut deps = mock_dependencies_with_balance(&[
        coin(10000000000u128, ORAI_DENOM),
        coin(20000000000u128, ATOM_DENOM),
    ]);

    let msg = InstantiateMsg {};
    let info = mock_info("addr", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    let _res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();

    //pair1
    let msg = ExecuteMsg::UpdatePair {
        from: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 18,
        },
        to: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset2"),
            },
            decimals: 6,
        },
        is_mint_burn: false,
    };

    //register pair1
    let info = mock_info("addr", &[]);
    execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

    //convert_reverse asset2 to asset1
    let info = mock_info("asset2", &[]);
    let convert_msg = Cw20HookMsg::ConvertReverse {
        from: AssetInfo::Token {
            contract_addr: Addr::unchecked("asset1"),
        },
    };
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(1u64),
        sender: info.sender.to_string(),
        msg: to_json_binary(&convert_msg).unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "asset1".to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: Uint128::from(10u128.pow(12))
            })
            .unwrap(),
            funds: vec![]
        }))]
    );

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "convert_token_reverse"),
            attr("from_amount", "1"),
            attr("to_amount", "1000000000000"),
        ]
    );

    //check if sender not from asset2
    let info = mock_info("addr", &[]);
    let convert_msg = Cw20HookMsg::ConvertReverse {
        from: AssetInfo::Token {
            contract_addr: Addr::unchecked("asset1"),
        },
    };
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(1u64),
        sender: info.sender.to_string(),
        msg: to_json_binary(&convert_msg).unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone());

    match res {
        Err(StdError::GenericErr { msg }) => assert_eq!(msg, "invalid cw20 hook message"),
        _ => panic!("Must return invalid cw20 hook message"),
    };

    //pair2
    let msg = ExecuteMsg::UpdatePair {
        from: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 6,
        },
        to: TokenInfo {
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.into(),
            },
            decimals: 18,
        },
        is_mint_burn: false,
    };
    let info = mock_info("addr", &[]);
    execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();

    //convert_reverse
    let msg = ExecuteMsg::ConvertReverse {
        from_asset: AssetInfo::Token {
            contract_addr: Addr::unchecked("asset1"),
        },
    };

    //convert 10^12 ORAI to asset1
    let info = mock_info("addr", &[coin(1000000000000u128, ORAI_DENOM)]);
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "asset1".to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: Uint128::from(1u128)
            })
            .unwrap(),
            funds: vec![]
        }))]
    );

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "convert_token_reverse"),
            attr("denom", ORAI_DENOM),
            attr("from_amount", "1000000000000"),
            attr("to_amount", "1"),
        ]
    );

    //check if not send Orai to convert to asset1
    let msg = ExecuteMsg::ConvertReverse {
        from_asset: AssetInfo::Token {
            contract_addr: Addr::unchecked("asset1"),
        },
    };

    //convert 10^12 ORAI to asset1
    let info = mock_info("addr", &[coin(1000000000000u128, ATOM_DENOM)]);
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone());

    match res {
        Err(StdError::GenericErr { msg }) => assert_eq!(
            msg,
            "Cannot find the native token that matches the input to convert in convert_reverse()"
        ),
        _ => panic!("Must return invalid cw20 hook message"),
    };
}

#[test]
fn test_remove_pair() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {};
    let info = mock_info("addr", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    let _res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();

    let msg = ExecuteMsg::UpdatePair {
        from: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 16,
        },
        to: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset2"),
            },
            decimals: 16,
        },
        is_mint_burn: false,
    };
    let info = mock_info("addr", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();

    let _res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::ConvertInfo {
            asset_info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
        },
    )
    .unwrap();

    let msg = ExecuteMsg::UnregisterPair {
        from: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 16,
        },
    };
    let info = mock_info("addr", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::ConvertInfo {
            asset_info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
        },
    );

    match res {
        Err(StdError::NotFound { kind }) => assert_eq!(kind, "oraiswap::converter::TokenRatio"),
        _ => panic!("Must return not found"),
    };
}

#[test]
fn test_withdraw_tokens() {
    let mut deps = mock_dependencies_with_balance(&[
        coin(10000000000u128, ORAI_DENOM),
        coin(20000000000u128, ATOM_DENOM),
    ]);

    let msg = InstantiateMsg {};
    let info = mock_info(
        "addr",
        &[
            coin(10000000000u128, ORAI_DENOM),
            coin(20000000000u128, ATOM_DENOM),
        ],
    );

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    //test proper withdraw tokens
    let msg = ExecuteMsg::WithdrawTokens {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: ORAI_DENOM.into(),
            },
            AssetInfo::NativeToken {
                denom: ATOM_DENOM.into(),
            },
        ],
    };

    let info = mock_info("addr", &[]);
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

    assert_eq!(
        res.messages,
        vec![
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: vec![coin(10000000000u128, ORAI_DENOM),],
            })),
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: vec![coin(20000000000u128, ATOM_DENOM),],
            }))
        ]
    );
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "withdraw_tokens"),
            attr("amount", 10000000000u128.to_string()),
            attr("amount", 20000000000u128.to_string())
        ]
    );

    //test unauthorized withdraw tokens
    let msg = ExecuteMsg::WithdrawTokens {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: ORAI_DENOM.into(),
            },
            AssetInfo::NativeToken {
                denom: ATOM_DENOM.into(),
            },
        ],
    };

    let info = mock_info("addr1", &[]);
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone());

    match res {
        Err(StdError::GenericErr { msg }) => assert_eq!(msg, "unauthorized"),
        _ => panic!("Must return unauthorized"),
    };
}

#[test]
fn test_convert_with_mint_burn_mechanism() {
    let mut deps = mock_dependencies_with_balance(&[
        coin(10000000000u128, ORAI_DENOM),
        coin(20000000000u128, ATOM_DENOM),
    ]);

    let msg = InstantiateMsg {};
    let info = mock_info("addr", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    let _res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();

    // pair native-cw20
    let msg = ExecuteMsg::UpdatePair {
        from: TokenInfo {
            info: AssetInfo::NativeToken {
                denom: ATOM_DENOM.to_string(),
            },
            decimals: 18,
        },
        to: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 6,
        },
        is_mint_burn: true,
    };
    let info = mock_info("addr", &[]);
    execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

    // try convert
    let msg = ExecuteMsg::Convert {};
    let res = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(
            "addr",
            &[Coin {
                denom: ATOM_DENOM.to_string(),
                amount: Uint128::from(1_000_000_000_000_000_000u128),
            }],
        ),
        msg,
    )
    .unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "asset1".to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Mint {
                recipient: "addr".to_string(),
                amount: Uint128::from(1000000u128)
            })
            .unwrap(),
            funds: vec![]
        }))]
    );

    // try convert reverse
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr".to_string(),
        amount: Uint128::from(1000000u128),
        msg: to_json_binary(&Cw20HookMsg::ConvertReverse {
            from: AssetInfo::NativeToken {
                denom: ATOM_DENOM.to_string(),
            },
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), mock_info("asset1", &[]), msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "addr".to_string(),
                amount: vec![Coin {
                    amount: Uint128::from(1_000_000_000_000_000_000u128),
                    denom: ATOM_DENOM.to_string()
                }],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset1".to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Burn {
                    amount: Uint128::from(1000000u128)
                })
                .unwrap(),
                funds: vec![]
            }))
        ]
    );

    // pair cw20-cw20
    let msg = ExecuteMsg::UpdatePair {
        from: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset2"),
            },
            decimals: 18,
        },
        to: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 6,
        },
        is_mint_burn: true,
    };
    let info = mock_info("addr", &[]);
    execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

    // try convert
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr".to_string(),
        amount: Uint128::from(1_000_000_000_000_000_000u128),
        msg: to_json_binary(&Cw20HookMsg::Convert {}).unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), mock_info("asset2", &[]), msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "asset1".to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Mint {
                recipient: "addr".to_string(),
                amount: Uint128::from(1000000u128)
            })
            .unwrap(),
            funds: vec![]
        }))]
    );

    // try convert reverse
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr".to_string(),
        amount: Uint128::from(1000000u128),
        msg: to_json_binary(&Cw20HookMsg::ConvertReverse {
            from: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset2"),
            },
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), mock_info("asset1", &[]), msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset2".to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "addr".to_string(),
                    amount: Uint128::from(1_000_000_000_000_000_000u128),
                })
                .unwrap(),
                funds: vec![]
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset1".to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Burn {
                    amount: Uint128::from(1000000u128)
                })
                .unwrap(),
                funds: vec![]
            }))
        ]
    );
}

#[test]
fn test_create_pair() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {};
    let info = mock_info("addr", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::UpdatePair {
        from: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 18,
        },
        to: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset2"),
            },
            decimals: 6,
        },
        is_mint_burn: false,
    };

    // create pair failed, unauthorized
    let info = mock_info("addr2", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg.clone());
    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "unauthorized"),
        _ => panic!("Must return unauthorized error"),
    }

    // register successful
    let info = mock_info("addr", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();

    let res: ConvertInfoResponse = from_json(
        &query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::ConvertInfo {
                asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset1"),
                },
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        res,
        ConvertInfoResponse {
            token_ratio: TokenRatio {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset2"),
                },
                ratio: Decimal::from_ratio(1u128, 1_000_000_000_000u128),
                is_mint_burn: false
            }
        }
    );

    // create pair with mint burn failed, to not be cw20

    let msg = ExecuteMsg::UpdatePair {
        from: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 18,
        },
        to: TokenInfo {
            info: AssetInfo::NativeToken {
                denom: ORAI_DENOM.to_string(),
            },
            decimals: 6,
        },
        is_mint_burn: true,
    };
    let info = mock_info("addr", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg.clone());
    match res {
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "With mint_burn mechanism, to_token must be cw20 token")
        }
        _ => panic!("Not enter here"),
    }

    // register pair with mint burn success
    let msg = ExecuteMsg::UpdatePair {
        from: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset1"),
            },
            decimals: 18,
        },
        to: TokenInfo {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset2"),
            },
            decimals: 6,
        },
        is_mint_burn: true,
    };
    let info = mock_info("addr", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();

    let res: ConvertInfoResponse = from_json(
        &query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::ConvertInfo {
                asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset1"),
                },
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        res,
        ConvertInfoResponse {
            token_ratio: TokenRatio {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset2"),
                },
                ratio: Decimal::from_ratio(1u128, 1_000_000_000_000u128),
                is_mint_burn: true
            }
        }
    );
}
