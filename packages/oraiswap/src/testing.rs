use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_schema::serde::Serialize;
use cosmwasm_std::{
    coin, Addr, AllBalanceResponse, Attribute, BalanceResponse, BankQuery, Coin, Decimal, Empty,
    QuerierWrapper, QueryRequest, StdResult, Uint128,
};
use std::collections::HashMap;

use crate::asset::{AssetInfo, PairInfo, ORAI_DENOM};

use crate::pair::DEFAULT_COMMISSION_RATE;
use cw_multi_test::{next_block, App, AppResponse, Contract, Executor};

pub const ATOM_DENOM: &str = "ibc/1777D03C5392415FE659F0E8ECB2CE553C6550542A68E4707D5D46949116790B";
pub const APP_OWNER: &str = "admin";

#[macro_export]
macro_rules! create_entry_points_testing {
    ($contract:ident) => {
        $crate::cw_multi_test::ContractWrapper::new(
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

pub struct MockApp {
    app: App,
    token_map: HashMap<String, Addr>, // map token name to address
    pub token_id: u64,
    pub oracle_addr: Addr,
    pub factory_addr: Addr,
}

impl MockApp {
    pub fn new(init_balances: &[(&String, &[Coin])]) -> Self {
        let app = App::new(|router, _, storage| {
            // init for App Owner a lot of balances
            router
                .bank
                .init_balance(
                    storage,
                    &Addr::unchecked(APP_OWNER),
                    vec![
                        coin(1000000000000000000u128, ORAI_DENOM),
                        coin(1000000000000000000u128, ATOM_DENOM),
                    ],
                )
                .unwrap();

            for (owner, init_funds) in init_balances.iter() {
                router
                    .bank
                    .init_balance(
                        storage,
                        &Addr::unchecked(owner.to_owned()),
                        init_funds.to_vec(),
                    )
                    .unwrap();
            }
        });

        MockApp {
            app,
            token_id: 0,
            oracle_addr: Addr::unchecked(""),
            factory_addr: Addr::unchecked(""),
            token_map: HashMap::new(),
        }
    }

    pub fn set_token_contract(&mut self, code: Box<dyn Contract<Empty>>) {
        self.token_id = self.upload(code);
    }

    pub fn upload(&mut self, code: Box<dyn Contract<Empty>>) -> u64 {
        let code_id = self.app.store_code(code);
        self.app.update_block(next_block);
        code_id
    }

    pub fn instantiate<T: Serialize>(
        &mut self,
        code_id: u64,
        sender: Addr,
        init_msg: &T,
        send_funds: &[Coin],
        label: &str,
    ) -> Result<Addr, String> {
        let contract_addr = self
            .app
            .instantiate_contract(
                code_id,
                sender.clone(),
                init_msg,
                send_funds,
                label,
                Some(sender.into_string()),
            )
            .map_err(|err| err.to_string())?;
        self.app.update_block(next_block);
        Ok(contract_addr)
    }

    pub fn execute<T: Serialize + std::fmt::Debug>(
        &mut self,
        sender: Addr,
        contract_addr: Addr,
        msg: &T,
        send_funds: &[Coin],
    ) -> Result<AppResponse, String> {
        let response = self
            .app
            .execute_contract(sender, contract_addr, msg, send_funds)
            .map_err(|err| err.to_string())?;

        self.app.update_block(next_block);

        Ok(response)
    }

    pub fn migrate<T: Serialize + std::fmt::Debug>(
        &mut self,
        sender: Addr,
        contract_addr: Addr,
        msg: &T,
        new_code_id: u64,
    ) -> Result<AppResponse, String> {
        let response = self
            .app
            .migrate_contract(sender, contract_addr, msg, new_code_id)
            .map_err(|err| err.to_string())?;

        self.app.update_block(next_block);

        Ok(response)
    }

    pub fn query<T: DeserializeOwned, U: Serialize>(
        &self,
        contract_addr: Addr,
        msg: &U,
    ) -> StdResult<T> {
        self.app.wrap().query_wasm_smart(contract_addr, msg)
    }

    pub fn set_oracle_contract(&mut self, code: Box<dyn Contract<Empty>>) {
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

    pub fn set_factory_and_pair_contract(
        &mut self,
        factory_code: Box<dyn Contract<Empty>>,
        pair_code: Box<dyn Contract<Empty>>,
    ) {
        let factory_id = self.upload(factory_code);
        let pair_code_id = self.upload(pair_code);

        self.factory_addr = self
            .instantiate(
                factory_id,
                Addr::unchecked(APP_OWNER),
                &crate::factory::InstantiateMsg {
                    pair_code_id,
                    token_code_id: self.token_id,
                    oracle_addr: self.oracle_addr.clone(),
                    commission_rate: Some(DEFAULT_COMMISSION_RATE.to_string()),
                },
                &[],
                "factory",
            )
            .unwrap();
    }

    // configure the oraiswap pair
    pub fn create_pairs(&mut self, asset_infos_list: &[[AssetInfo; 2]]) {
        for asset_infos in asset_infos_list.iter() {
            self.create_pair(asset_infos.clone());
        }
    }

    pub fn create_pair(&mut self, asset_infos: [AssetInfo; 2]) -> Option<Addr> {
        if !self.factory_addr.as_str().is_empty() {
            let res = self
                .execute(
                    Addr::unchecked(APP_OWNER),
                    self.factory_addr.clone(),
                    &crate::factory::ExecuteMsg::CreatePair {
                        asset_infos: asset_infos.clone(),
                        pair_admin: Some("admin".to_string()),
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
            let res = self
                .execute(
                    Addr::unchecked(APP_OWNER),
                    self.factory_addr.clone(),
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
            return self.app.wrap().query_wasm_smart(
                self.factory_addr.clone(),
                &crate::factory::QueryMsg::Pair { asset_infos },
            );
        }
        Err(cosmwasm_std::StdError::NotFound {
            kind: "Pair".into(),
        })
    }

    pub fn set_tax(&mut self, rate: Decimal, caps: &[(&String, &Uint128)]) {
        if !self.oracle_addr.as_str().is_empty() {
            // update rate
            self.execute(
                Addr::unchecked(APP_OWNER),
                self.oracle_addr.clone(),
                &crate::oracle::ExecuteMsg::UpdateTaxRate { rate },
                &[],
            )
            .unwrap();

            // update caps
            for (denom, &cap) in caps.iter() {
                self.execute(
                    Addr::unchecked(APP_OWNER),
                    self.oracle_addr.clone(),
                    &crate::oracle::ExecuteMsg::UpdateTaxCap {
                        denom: denom.to_string(),
                        cap: cap.clone(),
                    },
                    &[],
                )
                .unwrap();
            }
        }
    }

    pub fn query_balance(&self, account_addr: Addr, denom: String) -> StdResult<Uint128> {
        // load price form the oracle
        let balance: BalanceResponse =
            self.app
                .wrap()
                .query(&QueryRequest::Bank(BankQuery::Balance {
                    address: account_addr.to_string(),
                    denom,
                }))?;
        Ok(balance.amount.amount)
    }

    pub fn query_all_balances(&self, account_addr: Addr) -> StdResult<Vec<Coin>> {
        // load price form the oracle
        let all_balances: AllBalanceResponse =
            self.app
                .wrap()
                .query(&QueryRequest::Bank(BankQuery::AllBalances {
                    address: account_addr.to_string(),
                }))?;
        Ok(all_balances.amount)
    }

    pub fn register_token(&mut self, contract_addr: Addr) -> StdResult<String> {
        let res: cw20::TokenInfoResponse =
            self.query(contract_addr.clone(), &cw20::Cw20QueryMsg::TokenInfo {})?;
        self.token_map.insert(res.symbol.clone(), contract_addr);
        Ok(res.symbol)
    }

    pub fn query_token_balances(&self, account_addr: Addr) -> StdResult<Vec<Coin>> {
        let mut balances = vec![];
        for (denom, contract_addr) in self.token_map.iter() {
            let res: cw20::BalanceResponse = self.query(
                contract_addr.clone(),
                &cw20::Cw20QueryMsg::Balance {
                    address: account_addr.to_string(),
                },
            )?;
            balances.push(Coin {
                denom: denom.clone(),
                amount: res.balance,
            });
        }
        Ok(balances)
    }

    pub fn as_querier(&self) -> QuerierWrapper {
        self.app.wrap()
    }

    pub fn get_token_addr(&self, token: &str) -> Option<Addr> {
        self.token_map.get(token).cloned()
    }

    pub fn create_token(&mut self, token: &str) -> Addr {
        let addr = self
            .instantiate(
                self.token_id,
                Addr::unchecked(APP_OWNER),
                &cw20_base::msg::InstantiateMsg {
                    name: token.to_string(),
                    symbol: token.to_string(),
                    decimals: 6,
                    initial_balances: vec![],
                    mint: Some(cw20::MinterResponse {
                        minter: APP_OWNER.to_string(),
                        cap: None,
                    }),
                    marketing: None,
                },
                &[],
                "cw20",
            )
            .unwrap();
        self.token_map.insert(token.to_string(), addr.clone());
        addr
    }

    pub fn set_balances_from(
        &mut self,
        sender: Addr,
        balances: &[(&String, &[(&String, &Uint128)])],
    ) {
        for (denom, balances) in balances.iter() {
            // send for each recipient
            for (recipient, &amount) in balances.iter() {
                self.app
                    .send_tokens(
                        sender.clone(),
                        Addr::unchecked(recipient.as_str()),
                        &[Coin {
                            denom: denom.to_string(),
                            amount,
                        }],
                    )
                    .unwrap();
            }
        }
    }

    pub fn set_token_balances_from(
        &mut self,
        sender: Addr,
        balances: &[(&String, &[(&String, &Uint128)])],
    ) -> Vec<Addr> {
        let mut contract_addrs = vec![];
        for (token, balances) in balances.iter() {
            let contract_addr = match self.token_map.get(*token) {
                None => self.create_token(&token),
                Some(addr) => addr.clone(),
            };
            contract_addrs.push(contract_addr.clone());

            // mint for each recipient
            for (recipient, &amount) in balances.iter() {
                if !amount.is_zero() {
                    self.execute(
                        sender.clone(),
                        contract_addr.clone(),
                        &cw20::Cw20ExecuteMsg::Mint {
                            recipient: recipient.to_string(),
                            amount,
                        },
                        &[],
                    )
                    .unwrap();
                }
            }
        }
        contract_addrs
    }

    pub fn set_balances(&mut self, balances: &[(&String, &[(&String, &Uint128)])]) {
        self.set_balances_from(Addr::unchecked(APP_OWNER), balances)
    }

    // configure the mint whitelist mock querier
    pub fn set_token_balances(
        &mut self,
        balances: &[(&String, &[(&String, &Uint128)])],
    ) -> Vec<Addr> {
        self.set_token_balances_from(Addr::unchecked(APP_OWNER), balances)
    }

    pub fn assert_fail(&self, res: Result<AppResponse, String>) {
        // new version of cosmwasm does not return detail error
        match res.err() {
            Some(msg) => assert_eq!(msg.contains("error executing WasmMsg"), true),
            None => panic!("Must return generic error"),
        }
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

        app.set_token_balances(&[(
            &"AIRI".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(123u128))],
        )]);

        assert_eq!(
            Uint128::from(123u128),
            query_token_balance(
                &app.as_querier(),
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
            &"LPA".to_string(),
            &[
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(123u128)),
                (&"addr00000".to_string(), &Uint128::from(123u128)),
                (&"addr00001".to_string(), &Uint128::from(123u128)),
                (&"addr00002".to_string(), &Uint128::from(123u128)),
            ],
        )]);

        assert_eq!(
            query_supply(&app.as_querier(), app.get_token_addr("LPA").unwrap()).unwrap(),
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
            &"ASSET".to_string(),
            &[
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(123u128)),
                (&"addr00000".to_string(), &Uint128::from(123u128)),
                (&"addr00001".to_string(), &Uint128::from(123u128)),
                (&"addr00002".to_string(), &Uint128::from(123u128)),
            ],
        )]);

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
                .query_pool(&app.as_querier(), Addr::unchecked(MOCK_CONTRACT_ADDR))
                .unwrap(),
            Uint128::from(123u128)
        );
        assert_eq!(
            native_token_info
                .query_pool(&app.as_querier(), Addr::unchecked(MOCK_CONTRACT_ADDR))
                .unwrap(),
            Uint128::from(123u128)
        );
    }
}
