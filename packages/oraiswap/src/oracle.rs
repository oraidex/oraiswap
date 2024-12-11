use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_schema::{cw_serde, QueryResponses};

use cosmwasm_std::{
    to_json_binary, Addr, Api, CanonicalAddr, CosmosMsg, Decimal, QuerierWrapper, StdResult,
    Uint128, WasmMsg,
};

#[cw_serde]
pub struct InstantiateMsg {
    /// name of the NFT contract, can use default
    pub name: Option<String>,
    pub version: Option<String>,
    pub admin: Option<Addr>,
    pub min_rate: Option<Decimal>,
    pub max_rate: Option<Decimal>,
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateAdmin {
        admin: Addr,
    },
    UpdateExchangeRate {
        denom: String,
        exchange_rate: Decimal,
    },
    DeleteExchangeRate {
        denom: String,
    },
    UpdateTaxCap {
        denom: String,
        cap: Uint128,
    },
    // RateMax: 1%
    UpdateTaxRate {
        rate: Decimal,
    },
}

/// QueryMsg is defines available query datas
#[cw_serde]
#[derive(QueryResponses)]
#[query_responses(nested)]
pub enum QueryMsg {
    Treasury(OracleTreasuryQuery),
    Exchange(OracleExchangeQuery),
    Contract(OracleContractQuery),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum OracleTreasuryQuery {
    #[returns(TaxRateResponse)]
    TaxRate {},
    #[returns(TaxCapResponse)]
    TaxCap { denom: String },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum OracleExchangeQuery {
    #[returns(ExchangeRateResponse)]
    ExchangeRate {
        base_denom: Option<String>,
        quote_denom: String,
    },
    #[returns(ExchangeRatesResponse)]
    ExchangeRates {
        base_denom: Option<String>,
        quote_denoms: Vec<String>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum OracleContractQuery {
    #[returns(ContractInfoResponse)]
    ContractInfo {},
    #[returns(cosmwasm_std::Coin)]
    RewardPool { denom: String },
}

/// TaxRateResponse is data format returned from TreasuryRequest::TaxRate query
#[cw_serde]
pub struct TaxRateResponse {
    pub rate: Decimal,
}

/// TaxCapResponse is data format returned from TreasuryRequest::TaxCap query
#[cw_serde]
pub struct TaxCapResponse {
    pub cap: Uint128,
}

/// ExchangeRateItem is data format returned from OracleRequest::ExchangeRates query
#[cw_serde]
pub struct ExchangeRateItem {
    pub quote_denom: String,
    pub exchange_rate: Decimal,
}

/// ExchangeRatesResponse is data format returned from OracleRequest::ExchangeRates query
#[cw_serde]
pub struct ExchangeRatesResponse {
    pub base_denom: String,
    pub items: Vec<ExchangeRateItem>,
}

/// ExchangeRateResponse is data format returned from OracleRequest::ExchangeRate query
#[cw_serde]
pub struct ExchangeRateResponse {
    pub base_denom: String,
    pub item: ExchangeRateItem,
}

/// ContractInfo is data format stored
#[cw_serde]
pub struct ContractInfo {
    pub name: String,
    pub version: String,
    pub creator: CanonicalAddr,
    // admin can update the parameter, may be multisig
    pub admin: CanonicalAddr,
    // constraint
    pub min_rate: Decimal,
    pub max_rate: Decimal,
}

/// ContractInfoResponse is data format returned from WasmRequest::ContractInfo query
#[cw_serde]
pub struct ContractInfoResponse {
    pub name: String,
    pub version: String,
    pub creator: Addr,
    // admin can update the parameter, may be multisig
    pub admin: Addr,
    pub min_rate: Decimal,
    pub max_rate: Decimal,
}

/// We currently take no arguments for migrations
#[cw_serde]
pub struct MigrateMsg {}

/// OracleContract is a wrapper around Addr that provides a lot of helpers
/// for working with this.
///
/// If you wish to persist this, convert to Cw721CanonicalContract via .canonical()
#[cw_serde]
pub struct OracleContract(pub Addr);

impl OracleContract {
    pub fn addr(&self) -> Addr {
        self.0.clone()
    }

    /// Convert this address to a form fit for storage
    pub fn canonical<A: Api>(&self, api: &A) -> StdResult<OracleCanonicalContract> {
        let canon = api.addr_canonicalize(self.0.as_str())?;
        Ok(OracleCanonicalContract(canon))
    }

    pub fn call(&self, msg: ExecuteMsg) -> StdResult<CosmosMsg> {
        let msg = to_json_binary(&msg)?;
        Ok(WasmMsg::Execute {
            contract_addr: self.to_string(),
            msg,
            funds: vec![],
        }
        .into())
    }

    pub fn query<T: DeserializeOwned>(
        &self,
        querier: &QuerierWrapper,
        req: QueryMsg,
    ) -> StdResult<T> {
        querier.query_wasm_smart(self.to_string(), &req)
    }

    /*** queries ***/

    pub fn query_tax_cap<T: Into<String>>(
        &self,
        querier: &QuerierWrapper,
        denom: T,
    ) -> StdResult<TaxCapResponse> {
        let request = QueryMsg::Treasury(OracleTreasuryQuery::TaxCap {
            denom: denom.into(),
        });

        self.query(querier, request)
    }

    pub fn query_tax_rate(&self, querier: &QuerierWrapper) -> StdResult<TaxRateResponse> {
        let request = QueryMsg::Treasury(OracleTreasuryQuery::TaxRate {});

        self.query(querier, request)
    }

    // this is for CEX
    pub fn query_exchange_rate<T: Into<String>>(
        &self,
        querier: &QuerierWrapper,
        base_denom: T,
        quote_denom: T,
    ) -> StdResult<ExchangeRateResponse> {
        let request = QueryMsg::Exchange(OracleExchangeQuery::ExchangeRate {
            base_denom: Some(base_denom.into()),
            quote_denom: quote_denom.into(),
        });

        self.query(querier, request)
    }

    pub fn query_exchange_rates<T: Into<String>>(
        &self,
        querier: &QuerierWrapper,
        base_denom: T,
        quote_denoms: Vec<T>,
    ) -> StdResult<ExchangeRatesResponse> {
        let request = QueryMsg::Exchange(OracleExchangeQuery::ExchangeRates {
            base_denom: Some(base_denom.into()),
            quote_denoms: quote_denoms.into_iter().map(|x| x.into()).collect(),
        });

        self.query(querier, request)
    }

    pub fn query_contract_info<T: Into<String>>(
        &self,
        querier: &QuerierWrapper,
    ) -> StdResult<ContractInfoResponse> {
        let request = QueryMsg::Contract(OracleContractQuery::ContractInfo {});

        self.query(querier, request)
    }
}

impl ToString for OracleContract {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

/// This is a respresentation of OracleContract for storage.
/// Don't use it directly, just translate to the OracleContract when needed.
#[cw_serde]
pub struct OracleCanonicalContract(pub CanonicalAddr);

impl OracleCanonicalContract {
    /// Convert this address to a form fit for usage in messages and queries
    pub fn human<A: Api>(&self, api: &A) -> StdResult<OracleContract> {
        let human = api.addr_humanize(&self.0)?;
        Ok(OracleContract(human))
    }
}
