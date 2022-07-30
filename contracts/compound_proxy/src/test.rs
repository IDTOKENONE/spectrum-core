use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::{
    Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg,
};
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    coin, to_binary, Addr, Coin, CosmosMsg, Decimal, Order, StdResult, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Expiration};
use spectrum::compound_proxy::{CallbackMsg, ConfigResponse, ExecuteMsg, InstantiateMsg};

use crate::contract::{execute, instantiate, query_config};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use crate::state::PAIR_PROXY;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pair_contract: "pair_contract".to_string(),
        commission_bps: 30,
        pair_proxies: vec![
            (
                AssetInfo::Token {
                    contract_addr: Addr::unchecked("token0001"),
                },
                "pair0001".to_string(),
            ),
            (
                AssetInfo::NativeToken {
                    denom: "ibc/token".to_string(),
                },
                "pair0002".to_string(),
            ),
        ],
        slippage_tolerance: Decimal::percent(1),
    };

    let sender = "addr0000";

    let env = mock_env();
    let info = mock_info(sender, &[]);
    let res = instantiate(deps.as_mut(), env, info, msg);
    assert!(res.is_ok());

    let config = query_config(deps.as_ref()).unwrap();
    assert_eq!(
        config,
        ConfigResponse {
            pair_info: PairInfo {
                asset_infos: [
                    {
                        AssetInfo::Token {
                            contract_addr: Addr::unchecked("token"),
                        }
                    },
                    {
                        AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        }
                    }
                ],
                contract_addr: Addr::unchecked("pair_contract"),
                liquidity_token: Addr::unchecked("liquidity_token"),
                pair_type: astroport::factory::PairType::Xyk {}
            }
        }
    );

    let pair_proxies = PAIR_PROXY
        .range(&deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<(String, Addr)>>>()
        .unwrap();
    assert_eq!(
        pair_proxies,
        vec![
            ("ibc/token".to_string(), Addr::unchecked("pair0002")),
            ("token0001".to_string(), Addr::unchecked("pair0001")),
        ]
    )
}

#[test]
fn compound() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pair_contract: "pair_contract".to_string(),
        commission_bps: 30,
        pair_proxies: vec![],
        slippage_tolerance: Decimal::percent(1),
    };

    let sender = "addr0000";

    let env = mock_env();
    let info = mock_info(sender, &[]);
    let res = instantiate(deps.as_mut(), env, info, msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::Compound {
        rewards: vec![Asset {
            info: AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            amount: Uint128::from(1000000u128),
        }],
        to: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(1000000u128),
        }],
    );

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::Callback {
                    0: CallbackMsg::OptimalSwap {}
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::Callback {
                    0: CallbackMsg::ProvideLiquidity {
                        receiver: "addr0000".to_string()
                    }
                })
                .unwrap(),
            }),
        ]
    );
}

#[test]
fn optimal_swap() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance(&[(
        &String::from("pair_contract"),
        &[Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(1000000000),
        }],
    )]);
    deps.querier.with_token_balances(&[(
        &String::from("token"),
        &[
            (&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(1000000)),
            (&String::from("pair_contract"), &Uint128::new(1000000000)),
        ],
    )]);

    let env = mock_env();

    let msg = InstantiateMsg {
        pair_contract: "pair_contract".to_string(),
        commission_bps: 30,
        pair_proxies: vec![],
        slippage_tolerance: Decimal::percent(1),
    };

    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::Callback {
        0: CallbackMsg::OptimalSwap {},
    };

    let res = execute(deps.as_mut(), env.clone().clone(), info, msg.clone());
    assert_eq!(res, Err(ContractError::Unauthorized {}));

    let info = mock_info(env.contract.address.as_str(), &[]);
    let res = execute(deps.as_mut(), env.clone().clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "token".to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: "pair_contract".to_string(),
                amount: Uint128::new(500626),
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })
                .unwrap()
            })
            .unwrap(),
        }),]
    );
}

#[test]
fn provide_liquidity() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance(&[
        (
            &String::from("pair_contract"),
            &[Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(1000000000),
            }],
        ),
        (
            &String::from(MOCK_CONTRACT_ADDR),
            &[Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(1000000),
            }],
        ),
    ]);
    deps.querier.with_token_balances(&[(
        &String::from("token"),
        &[
            (&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(1000000)),
            (&String::from("pair_contract"), &Uint128::new(1000000000)),
        ],
    )]);

    let env = mock_env();

    let msg = InstantiateMsg {
        pair_contract: "pair_contract".to_string(),
        commission_bps: 30,
        pair_proxies: vec![],
        slippage_tolerance: Decimal::percent(1),
    };

    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::Callback {
        0: CallbackMsg::ProvideLiquidity {
            receiver: "sender".to_string(),
        },
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_eq!(res, Err(ContractError::Unauthorized {}));

    let info = mock_info(env.contract.address.as_str(), &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "token".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: "pair_contract".to_string(),
                    amount: Uint128::from(1000000u128),
                    expires: Some(Expiration::AtHeight(12346)),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair_contract".to_string(),
                funds: vec![coin(1000000, "uluna")],
                msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: Addr::unchecked("token"),
                            },
                            amount: Uint128::from(1000000u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uluna".to_string(),
                            },
                            amount: Uint128::from(1000000u128),
                        },
                    ],
                    slippage_tolerance: Some(Decimal::percent(1)),
                    auto_stake: None,
                    receiver: Some("sender".to_string()),
                })
                .unwrap(),
            }),
        ]
    );
}
