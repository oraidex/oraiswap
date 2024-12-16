use crate::{
    asset::{Asset, AssetInfo, PairInfo, ORAI_DENOM},
    factory::ProvideLiquidityParams,
};
use cosmwasm_std::{coin, Addr, Attribute, Coin, Decimal, StdResult, Uint128};
use derive_more::{Deref, DerefMut};
use oraiswap_v3::percentage::Percentage;

use crate::pair::{DEFAULT_COMMISSION_RATE, DEFAULT_OPERATOR_FEE};
use cosmwasm_testing_util::{
    error::{AnyError, Error},
    AppResponse, Code, MockResult,
};

pub const ATOM_DENOM: &str = "ibc/1777D03C5392415FE659F0E8ECB2CE553C6550542A68E4707D5D46949116790B";
pub const APP_OWNER: &str = "admin";

#[macro_export]
macro_rules! create_entry_points_testing {
    ($contract:ident) => {
        $crate::cosmwasm_testing_util::ContractWrapper::new_with_empty(
            $contract::contract::execute,
            $contract::contract::instantiate,
            $contract::contract::query,
        )
    };
}

pub trait AttributeUtil {
    fn get_attributes(&self, index: usize) -> Vec<Attribute>;
}

impl AttributeUtil for AppResponse {
    fn get_attributes(&self, index: usize) -> Vec<Attribute> {
        self.events[index].attributes[1..].to_vec()
    }
}

#[derive(Deref, DerefMut)]
pub struct MockApp {
    #[deref]
    #[deref_mut]
    app: cosmwasm_testing_util::MockApp,
    pub oracle_addr: Addr,
    pub factory_addr: Addr,
    pub router_addr: Addr,
    pub v3_addr: Addr,
}

impl MockApp {
    pub fn new(init_balances: &[(&str, &[Coin])]) -> Self {
        let mut init_balances = init_balances.to_vec();
        let owner_balances = vec![
            coin(1000000000000000000u128, ORAI_DENOM),
            coin(1000000000000000000u128, ATOM_DENOM),
        ];
        init_balances.push((APP_OWNER, &owner_balances));
        let app = cosmwasm_testing_util::MockApp::new(&init_balances);

        MockApp {
            app,
            oracle_addr: Addr::unchecked(""),
            factory_addr: Addr::unchecked(""),
            router_addr: Addr::unchecked(""),
            v3_addr: Addr::unchecked(""),
        }
    }

    pub fn set_oracle_contract(&mut self, code: Code) {
        let code_id = self.upload(code);
        self.oracle_addr = self
            .instantiate(
                code_id,
                Addr::unchecked(APP_OWNER),
                &crate::oracle::InstantiateMsg {
                    name: None,
                    version: None,
                    admin: None,
                    min_rate: None,
                    max_rate: None,
                },
                &[],
                "oracle",
            )
            .unwrap();
    }

    pub fn set_factory_and_pair_contract(&mut self, factory_code: Code, pair_code: Code) {
        let factory_id = self.upload(factory_code);
        let pair_code_id = self.upload(pair_code);
        let token_code_id = self.token_id();
        let oracle_addr = self.oracle_addr.clone();
        self.factory_addr = self
            .instantiate(
                factory_id,
                Addr::unchecked(APP_OWNER),
                &crate::factory::InstantiateMsg {
                    pair_code_id,
                    token_code_id,
                    oracle_addr,
                    commission_rate: Some(DEFAULT_COMMISSION_RATE.to_string()),
                    operator_fee: Some(DEFAULT_OPERATOR_FEE.to_string()),
                },
                &[],
                "factory",
            )
            .unwrap();
    }

    pub fn set_router_contract(&mut self, code: Code, factory_addr: Addr) {
        let code_id = self.upload(code);
        self.router_addr = self
            .instantiate(
                code_id,
                Addr::unchecked(APP_OWNER),
                &crate::router::InstantiateMsg {
                    factory_addr: factory_addr.clone(),
                    factory_addr_v2: factory_addr.clone(),
                },
                &[],
                "router",
            )
            .unwrap();
    }

    // configure the oraiswap pair
    pub fn create_pairs(&mut self, asset_infos_list: &[[AssetInfo; 2]]) {
        for asset_infos in asset_infos_list.iter() {
            self.create_pair(asset_infos.clone());
        }
    }

    pub fn create_token(&mut self, token: &str) -> Addr {
        self.app.create_token(APP_OWNER, token, 0u128)
    }

    pub fn increase_allowance(
        &mut self,
        token: Addr,
        amount: u128,
    ) -> Result<AppResponse, AnyError> {
        self.app.execute(
            Addr::unchecked(APP_OWNER),
            token,
            &cw20::Cw20ExecuteMsg::IncreaseAllowance {
                spender: self.factory_addr.to_string(),
                amount: Uint128::from(amount),
                expires: None,
            },
            &[],
        )
    }

    pub fn mint_token(&mut self, token: Addr, amount: u128) -> Result<AppResponse, AnyError> {
        self.app.execute(
            Addr::unchecked(APP_OWNER),
            token,
            &cw20::Cw20ExecuteMsg::Mint {
                recipient: APP_OWNER.to_string(),
                amount: Uint128::from(amount),
            },
            &[],
        )
    }

    pub fn set_balances(&mut self, balances: &[(&str, &[(&str, u128)])]) {
        self.app.set_balances(APP_OWNER, balances)
    }

    pub fn create_pair(&mut self, asset_infos: [AssetInfo; 2]) -> Option<Addr> {
        if !self.factory_addr.as_str().is_empty() {
            let contract_addr = self.factory_addr.clone();
            let res = self
                .execute(
                    Addr::unchecked(APP_OWNER),
                    contract_addr,
                    &crate::factory::ExecuteMsg::CreatePair {
                        asset_infos: asset_infos.clone(),
                        pair_admin: Some("admin".to_string()),
                        operator: Some("operator".to_string()),
                        provide_liquidity: None,
                    },
                    &[],
                )
                .unwrap();

            for event in res.events {
                for attr in event.attributes {
                    if attr.key.eq("pair_contract_address") {
                        return Some(Addr::unchecked(attr.value));
                    }
                }
            }
        }
        None
    }

    pub fn create_pair_add_add_liquidity(&mut self, asset_infos: [AssetInfo; 2]) -> Option<Addr> {
        if !self.factory_addr.as_str().is_empty() {
            let contract_addr = self.factory_addr.clone();
            let res = self
                .execute(
                    Addr::unchecked(APP_OWNER),
                    contract_addr,
                    &crate::factory::ExecuteMsg::CreatePair {
                        asset_infos: asset_infos.clone(),
                        pair_admin: Some("admin".to_string()),
                        operator: Some("operator".to_string()),
                        provide_liquidity: Some(ProvideLiquidityParams {
                            assets: [
                                Asset {
                                    info: asset_infos[0].clone(),
                                    amount: Uint128::from(1000000u128),
                                },
                                Asset {
                                    info: asset_infos[1].clone(),
                                    amount: Uint128::from(1000000u128),
                                },
                            ],
                            receiver: None,
                        }),
                    },
                    &[],
                )
                .unwrap();

            for event in res.events {
                for attr in event.attributes {
                    if attr.key.eq("pair_contract_address") {
                        return Some(Addr::unchecked(attr.value));
                    }
                }
            }
        }
        None
    }

    pub fn add_pair(&mut self, pair_info: PairInfo) -> Option<String> {
        if !self.factory_addr.as_str().is_empty() {
            let contract_addr = self.factory_addr.clone();
            let res = self
                .execute(
                    Addr::unchecked(APP_OWNER),
                    contract_addr,
                    &crate::factory::ExecuteMsg::AddPair { pair_info },
                    &[],
                )
                .unwrap();

            for event in res.events {
                for attr in event.attributes {
                    if attr.value.eq("add_pair") {
                        return Some(attr.value);
                    }
                }
            }
        }

        None
    }

    pub fn query_pair(&self, asset_infos: [AssetInfo; 2]) -> StdResult<PairInfo> {
        if !self.factory_addr.as_str().is_empty() {
            return self.app.as_querier().query_wasm_smart(
                self.factory_addr.clone(),
                &crate::factory::QueryMsg::Pair { asset_infos },
            );
        }
        Err(cosmwasm_std::StdError::NotFound {
            kind: "Pair".into(),
        })
    }

    // configure the mint whitelist mock querier
    pub fn set_token_balances(
        &mut self,
        balances: &[(&str, &[(&str, u128)])],
    ) -> MockResult<Vec<Addr>> {
        self.set_token_balances_from(APP_OWNER, balances)
    }

    pub fn set_tax(&mut self, rate: Decimal, caps: &[(&str, u128)]) {
        if !self.oracle_addr.as_str().is_empty() {
            let contract_addr = self.oracle_addr.clone();
            // update rate
            self.execute(
                Addr::unchecked(APP_OWNER),
                contract_addr.clone(),
                &crate::oracle::ExecuteMsg::UpdateTaxRate { rate },
                &[],
            )
            .unwrap();

            // update caps
            for (denom, cap) in caps.iter() {
                self.execute(
                    Addr::unchecked(APP_OWNER),
                    contract_addr.clone(),
                    &crate::oracle::ExecuteMsg::UpdateTaxCap {
                        denom: denom.to_string(),
                        cap: Uint128::from(*cap),
                    },
                    &[],
                )
                .unwrap();
            }
        }
    }

    pub fn create_v3(&mut self, code: Code) {
        let code_id = self.upload(code);
        self.v3_addr = self
            .instantiate(
                code_id,
                Addr::unchecked(APP_OWNER),
                &oraiswap_v3::msg::InstantiateMsg {
                    protocol_fee: Percentage(0),
                },
                &[],
                "router",
            )
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{testing::MOCK_CONTRACT_ADDR, Addr, Coin, Uint128};

    use crate::{
        asset::AssetInfo,
        querier::{query_supply, query_token_balance},
        testing::MockApp,
    };

    #[test]
    fn token_balance_querier() {
        let mut app = MockApp::new(&[]);

        app.set_token_contract(Box::new(crate::create_entry_points_testing!(cw20_base)));

        app.set_token_balances(&[("AIRI", &[(&MOCK_CONTRACT_ADDR.to_string(), 123u128)])])
            .unwrap();

        assert_eq!(
            Uint128::from(123u128),
            query_token_balance(
                &app.as_querier().into_empty(),
                app.get_token_addr("AIRI").unwrap(),
                Addr::unchecked(MOCK_CONTRACT_ADDR),
            )
            .unwrap()
        );
    }

    #[test]
    fn balance_querier() {
        let app = MockApp::new(&[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &[Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(200u128),
            }],
        )]);

        assert_eq!(
            app.query_balance(Addr::unchecked(MOCK_CONTRACT_ADDR), "uusd".to_string())
                .unwrap(),
            Uint128::from(200u128)
        );
    }

    #[test]
    fn all_balances_querier() {
        let app = MockApp::new(&[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &[
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(200u128),
                },
                Coin {
                    denom: "ukrw".to_string(),
                    amount: Uint128::from(300u128),
                },
            ],
        )]);

        let mut balance1 = app
            .query_all_balances(Addr::unchecked(MOCK_CONTRACT_ADDR))
            .unwrap();
        balance1.sort_by(|a, b| a.denom.cmp(&b.denom));
        let mut balance2 = vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(200u128),
            },
            Coin {
                denom: "ukrw".to_string(),
                amount: Uint128::from(300u128),
            },
        ];
        balance2.sort_by(|a, b| a.denom.cmp(&b.denom));
        assert_eq!(balance1, balance2);
    }

    #[test]
    fn supply_querier() {
        let mut app = MockApp::new(&[]);
        app.set_token_contract(Box::new(crate::create_entry_points_testing!(cw20_base)));
        app.set_token_balances(&[(
            "LPA",
            &[
                (&MOCK_CONTRACT_ADDR.to_string(), 123u128),
                ("addr00000", 123u128),
                ("addr00001", 123u128),
                ("addr00002", 123u128),
            ],
        )])
        .unwrap();

        assert_eq!(
            query_supply(
                &app.as_querier().into_empty(),
                app.get_token_addr("LPA").unwrap()
            )
            .unwrap(),
            Uint128::from(492u128)
        )
    }

    #[test]
    fn test_asset_info() {
        let mut app = MockApp::new(&[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &[Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(123u128),
            }],
        )]);
        app.set_token_contract(Box::new(crate::create_entry_points_testing!(cw20_base)));

        app.set_token_balances(&[(
            "ASSET",
            &[
                (&MOCK_CONTRACT_ADDR.to_string(), 123u128),
                ("addr00000", 123u128),
                ("addr00001", 123u128),
                ("addr00002", 123u128),
            ],
        )])
        .unwrap();

        let token_info = AssetInfo::Token {
            contract_addr: app.get_token_addr("ASSET").unwrap(),
        };
        let native_token_info = AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        };

        assert!(!token_info.eq(&native_token_info));
        assert!(native_token_info.is_native_token());
        assert!(!token_info.is_native_token());

        assert_eq!(
            token_info
                .query_pool(
                    &app.as_querier().into_empty(),
                    Addr::unchecked(MOCK_CONTRACT_ADDR)
                )
                .unwrap(),
            Uint128::from(123u128)
        );
        assert_eq!(
            native_token_info
                .query_pool(
                    &app.as_querier().into_empty(),
                    Addr::unchecked(MOCK_CONTRACT_ADDR)
                )
                .unwrap(),
            Uint128::from(123u128)
        );
    }
}
