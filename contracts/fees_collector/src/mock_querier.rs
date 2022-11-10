use std::collections::HashMap;
use cosmwasm_std::{Addr, BalanceResponse, BankQuery, Binary, Coin, ContractResult, Decimal, Empty, from_binary, from_slice, OwnedDeps, Querier, QuerierResult, QueryRequest, StdError, StdResult, SystemError, SystemResult, to_binary, Uint128, WasmQuery};
use cosmwasm_std::testing::{MockApi, MockStorage};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use astroport::asset::{token_asset, AssetInfo, PairInfo, Asset};
use astroport::generator::{PendingTokenResponse};
use astroport::pair::SimulationResponse;

pub fn mock_dependencies() -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new();

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
        custom_query_type: Default::default()
    }
}

const ASTRO_TOKEN: &str = "astro";
const REWARD_TOKEN: &str = "reward";

pub struct WasmMockQuerier {
    balances: HashMap<(String, String), Uint128>,
    raw: HashMap<(String, Binary), Binary>,
    pairs: HashMap<Vec<u8>, PairInfo>,
    prices: HashMap<String, Decimal>,
}

impl WasmMockQuerier {
    pub fn new() -> Self {
        WasmMockQuerier {
            balances: HashMap::new(),
            raw: HashMap::new(),
            pairs: HashMap::new(),
            prices: HashMap::new(),
        }
    }

    pub fn set_balance(&mut self, token: String, addr: String, amount: Uint128) {
        self.balances.insert((token, addr), amount);
    }

    fn get_balance(&self, token: String, addr: String) -> Uint128 {
        *self.balances.get(&(token, addr)).unwrap_or(&Uint128::zero())
    }

    pub fn set_pair(&mut self, asset_infos: &[AssetInfo; 2], pair_info: PairInfo) {
        self.pairs.insert(pair_key(&asset_infos), pair_info);
    }

    fn get_pair(&self, asset_infos: &[AssetInfo; 2]) -> Option<&PairInfo> {
        self.pairs.get(&pair_key(&asset_infos))
    }


    pub fn set_price(&mut self, pair: String, price: Decimal) {
        self.prices.insert(pair, price);
    }

    fn get_price(&self, pair: &String) -> Option<&Decimal> {
        self.prices.get(pair)
    }

    fn execute_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        let result = match request {
            QueryRequest::Bank(BankQuery::Balance {
                                   address,
                                   denom,
                               }) => {
                let amount = self.get_balance(denom.clone(), address.clone());
                to_binary(&BalanceResponse {
                    amount: Coin {
                        denom: denom.clone(),
                        amount,
                    },
                })
            },
            QueryRequest::Wasm(WasmQuery::Smart {
                                   contract_addr,
                                   msg,
                               }) => self.execute_wasm_query(contract_addr, msg),
            QueryRequest::Wasm(WasmQuery::Raw {
                                   contract_addr,
                                   key,
                               }) => {
                let value = self.raw.get(&(contract_addr.clone(), key.clone()));
                if let Some(binary) = value {
                    Ok(binary.clone())
                } else {
                    Ok(Binary::default())
                }
            },
            _ => return QuerierResult::Err(SystemError::Unknown {}),
        };
        QuerierResult::Ok(ContractResult::from(result))
    }

    fn execute_wasm_query(&self, contract_addr: &String, msg: &Binary) -> StdResult<Binary> {
        match from_binary(msg)? {
            MockQueryMsg::Balance {
                address,
            } => {
                let balance = self.get_balance(contract_addr.clone(), address);
                to_binary(&cw20::BalanceResponse {
                    balance,
                })
            },
            MockQueryMsg::Deposit {
                lp_token,
                ..
            } => {
                let balance = self.get_balance(contract_addr.clone(), lp_token);
                to_binary(&balance)
            },
            MockQueryMsg::PendingToken { .. } => {
                let pending = self.get_balance(contract_addr.clone(), ASTRO_TOKEN.to_string());
                let reward = self.get_balance(contract_addr.clone(), REWARD_TOKEN.to_string());
                to_binary(&PendingTokenResponse {
                    pending,
                    pending_on_proxy: Some(vec![
                        token_asset(Addr::unchecked(REWARD_TOKEN), reward),
                    ]),
                })
            },
            MockQueryMsg::Pair {
                asset_infos,
            } => {
                let pair_info = self.get_pair(&asset_infos)
                    .ok_or_else(|| StdError::generic_err("No pair info"))?;
                to_binary(pair_info)
             },
            MockQueryMsg::Simulation { offer_asset } => {
                let price = *self.get_price(contract_addr)
                    .ok_or_else(|| StdError::generic_err("No price"))?;
                to_binary(&SimulationResponse {
                    return_amount: offer_asset.amount * price,
                    spread_amount: Default::default(),
                    commission_amount: Default::default()
                })
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MockQueryMsg {
    Balance {
        address: String,
    },
    Deposit {
        lp_token: String,
        user: String,
    },
    PendingToken {
        lp_token: String,
        user: String
    },
    Pair {
        asset_infos: [AssetInfo; 2],
    },
    Simulation {
        offer_asset: Asset,
    },
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.execute_query(&request)
    }
}

fn pair_key(asset_infos: &[AssetInfo; 2]) -> Vec<u8> {
    let mut asset_infos = asset_infos.to_vec();
    asset_infos.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

    [asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat()
}
