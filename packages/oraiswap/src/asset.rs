use cosmwasm_schema::cw_serde;
use std::fmt;

use crate::querier::query_token_balance;
use crate::{error::ContractError, oracle::OracleContract};

use cosmwasm_std::{
    coin, to_json_binary, Addr, Api, BankMsg, CanonicalAddr, CosmosMsg, Decimal, MessageInfo,
    QuerierWrapper, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

pub const ORAI_DENOM: &str = "orai";

#[cw_serde]
pub struct Asset {
    pub info: AssetInfo,
    pub amount: Uint128,
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.amount, self.info)
    }
}

impl Asset {
    pub fn is_native_token(&self) -> bool {
        self.info.is_native_token()
    }

    pub fn compute_tax(
        &self,
        oracle_contract: &OracleContract,
        querier: &QuerierWrapper,
    ) -> StdResult<Uint128> {
        let amount = self.amount;
        if let AssetInfo::NativeToken { denom } = &self.info {
            if denom == ORAI_DENOM {
                Ok(Uint128::from(0u64))
            } else {
                // get oracle params from oracle contract
                let tax_rate = oracle_contract.query_tax_rate(querier)?.rate;
                let tax_cap = oracle_contract
                    .query_tax_cap(querier, denom.to_string())?
                    .cap;

                Ok(std::cmp::min(
                    amount.checked_sub(amount * (Decimal::one() / (tax_rate + Decimal::one())))?,
                    tax_cap,
                ))
            }
        } else {
            Ok(Uint128::from(0u64))
        }
    }

    /// create a CosmosMsg send message to receiver
    pub fn into_msg(
        &self,
        oracle_contract: Option<&OracleContract>,
        querier: &QuerierWrapper,
        recipient: Addr,
    ) -> StdResult<CosmosMsg> {
        let amount = self.amount;

        match &self.info {
            AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient.to_string(),
                    amount,
                })?,
                funds: vec![],
            })),
            AssetInfo::NativeToken { denom } => {
                // if there is oracle contract then calculate tax deduction
                let send_amount = if let Some(oracle_contract) = oracle_contract {
                    coin(
                        self.amount
                            .checked_sub(self.compute_tax(oracle_contract, querier)?)?
                            .into(),
                        denom,
                    )
                } else {
                    coin(amount.u128(), denom)
                };
                Ok(CosmosMsg::Bank(BankMsg::Send {
                    to_address: recipient.to_string(),
                    amount: vec![send_amount],
                }))
            }
        }
    }

    pub fn assert_sent_native_token_balance(&self, message_info: &MessageInfo) -> StdResult<()> {
        if let AssetInfo::NativeToken { denom } = &self.info {
            match message_info.funds.iter().find(|x| x.denom.eq(denom)) {
                Some(coin) => {
                    if self.amount == coin.amount {
                        Ok(())
                    } else {
                        Err(StdError::generic_err("Native token balance mismatch between the argument and the transferred"))
                    }
                }
                None => {
                    if self.amount.is_zero() {
                        Ok(())
                    } else {
                        Err(StdError::generic_err("Native token balance mismatch between the argument and the transferred"))
                    }
                }
            }
        } else {
            Ok(())
        }
    }

    pub fn assert_if_asset_is_native_token(&self) -> StdResult<()> {
        // if paid asset is cw20, we check it in Cw20HookMessage
        if !self.is_native_token() {
            return Err(StdError::generic_err(
                ContractError::MustProvideNativeToken {}.to_string(),
            ));
        }
        Ok(())
    }

    pub fn assert_if_asset_is_zero(&self) -> StdResult<()> {
        // if paid asset is cw20, we check it in Cw20HookMessage
        if self.amount.is_zero() {
            return Err(StdError::generic_err(
                ContractError::AssetMustNotBeZero {}.to_string(),
            ));
        }
        Ok(())
    }

    pub fn to_raw(&self, api: &dyn Api) -> StdResult<AssetRaw> {
        Ok(AssetRaw {
            info: match &self.info {
                AssetInfo::NativeToken { denom } => AssetInfoRaw::NativeToken {
                    denom: denom.to_string(),
                },
                AssetInfo::Token { contract_addr } => AssetInfoRaw::Token {
                    contract_addr: api.addr_canonicalize(contract_addr.as_str())?,
                },
            },
            amount: self.amount,
        })
    }
}

/// AssetInfo contract_addr is usually passed from the cw20 hook
/// so we can trust the contract_addr is properly validated.
#[cw_serde]
pub enum AssetInfo {
    Token { contract_addr: Addr },
    NativeToken { denom: String },
}

impl fmt::Display for AssetInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AssetInfo::NativeToken { denom } => write!(f, "{}", denom),
            AssetInfo::Token { contract_addr } => write!(f, "{}", contract_addr),
        }
    }
}

impl AssetInfo {
    pub fn from_denom(api: &dyn Api, denom: &str) -> Self {
        if let Ok(contract_addr) = api.addr_validate(denom) {
            Self::Token { contract_addr }
        } else {
            Self::NativeToken {
                denom: denom.to_string(),
            }
        }
    }

    pub fn to_vec(&self, api: &dyn Api) -> StdResult<Vec<u8>> {
        match self {
            AssetInfo::NativeToken { denom } => Ok(denom.as_bytes().to_vec()),
            AssetInfo::Token { contract_addr } => api
                .addr_canonicalize(contract_addr.as_str())
                .map(|addr| addr.as_slice().to_vec()),
        }
    }

    pub fn to_raw(&self, api: &dyn Api) -> StdResult<AssetInfoRaw> {
        match self {
            AssetInfo::NativeToken { denom } => Ok(AssetInfoRaw::NativeToken {
                denom: denom.to_string(),
            }),
            AssetInfo::Token { contract_addr } => Ok(AssetInfoRaw::Token {
                contract_addr: api.addr_canonicalize(contract_addr.as_str())?,
            }),
        }
    }

    pub fn is_native_token(&self) -> bool {
        match self {
            AssetInfo::NativeToken { .. } => true,
            AssetInfo::Token { .. } => false,
        }
    }
    pub fn query_pool(&self, querier: &QuerierWrapper, pool_addr: Addr) -> StdResult<Uint128> {
        match self {
            AssetInfo::Token { contract_addr, .. } => {
                query_token_balance(querier, contract_addr.to_owned(), pool_addr)
            }
            AssetInfo::NativeToken { denom, .. } => {
                Ok(querier.query_balance(pool_addr, denom)?.amount)
            }
        }
    }

    pub fn eq(&self, asset: &AssetInfo) -> bool {
        match self {
            AssetInfo::Token { contract_addr, .. } => {
                let self_contract_addr = contract_addr;
                match asset {
                    AssetInfo::Token { contract_addr, .. } => self_contract_addr == contract_addr,
                    AssetInfo::NativeToken { .. } => false,
                }
            }
            AssetInfo::NativeToken { denom, .. } => {
                let self_denom = denom;
                match asset {
                    AssetInfo::Token { .. } => false,
                    AssetInfo::NativeToken { denom, .. } => self_denom == denom,
                }
            }
        }
    }
}

#[cw_serde]
pub struct AssetRaw {
    pub info: AssetInfoRaw,
    pub amount: Uint128,
}

impl AssetRaw {
    pub fn to_normal(&self, api: &dyn Api) -> StdResult<Asset> {
        Ok(Asset {
            info: match &self.info {
                AssetInfoRaw::NativeToken { denom } => AssetInfo::NativeToken {
                    denom: denom.to_string(),
                },
                AssetInfoRaw::Token { contract_addr } => AssetInfo::Token {
                    contract_addr: api.addr_humanize(contract_addr)?,
                },
            },
            amount: self.amount,
        })
    }
}

#[cw_serde]
pub enum AssetInfoRaw {
    #[serde(alias = "Token")]
    Token { contract_addr: CanonicalAddr },
    #[serde(alias = "NativeToken")]
    NativeToken { denom: String },
}

impl AssetInfoRaw {
    pub fn to_normal(&self, api: &dyn Api) -> StdResult<AssetInfo> {
        match self {
            AssetInfoRaw::NativeToken { denom } => Ok(AssetInfo::NativeToken {
                denom: denom.to_string(),
            }),
            AssetInfoRaw::Token { contract_addr } => Ok(AssetInfo::Token {
                contract_addr: api.addr_humanize(contract_addr)?,
            }),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            AssetInfoRaw::NativeToken { denom } => denom.as_bytes(),
            AssetInfoRaw::Token { contract_addr } => contract_addr.as_slice(),
        }
    }

    pub fn eq(&self, asset: &AssetInfoRaw) -> bool {
        match self {
            AssetInfoRaw::Token { contract_addr, .. } => {
                let self_contract_addr = contract_addr;
                match asset {
                    AssetInfoRaw::Token { contract_addr, .. } => {
                        self_contract_addr == contract_addr
                    }
                    AssetInfoRaw::NativeToken { .. } => false,
                }
            }
            AssetInfoRaw::NativeToken { denom, .. } => {
                let self_denom = denom;
                match asset {
                    AssetInfoRaw::Token { .. } => false,
                    AssetInfoRaw::NativeToken { denom, .. } => self_denom == denom,
                }
            }
        }
    }
}

// We define a custom struct for each query response
#[cw_serde]
pub struct PairInfo {
    pub asset_infos: [AssetInfo; 2],
    pub contract_addr: Addr,
    pub liquidity_token: Addr,

    pub oracle_addr: Addr,
    pub commission_rate: String, // commission rate as swap fee, and will ve devided for liquidity provider
    #[serde(default)]
    pub operator_fee: String, // operator fee send to system
}

#[cw_serde]
pub struct PairInfoRaw {
    pub asset_infos: [AssetInfoRaw; 2],
    pub contract_addr: CanonicalAddr,
    pub liquidity_token: CanonicalAddr,

    // oracle contract
    pub oracle_addr: CanonicalAddr,
    pub commission_rate: String,
    #[serde(default)]
    pub operator_fee: String,
}

impl PairInfoRaw {
    pub fn to_normal(&self, api: &dyn Api) -> StdResult<PairInfo> {
        Ok(PairInfo {
            liquidity_token: api.addr_humanize(&self.liquidity_token)?,
            contract_addr: api.addr_humanize(&self.contract_addr)?,
            oracle_addr: api.addr_humanize(&self.oracle_addr)?,
            asset_infos: [
                self.asset_infos[0].to_normal(api)?,
                self.asset_infos[1].to_normal(api)?,
            ],
            commission_rate: self.commission_rate.clone(),
            operator_fee: self.operator_fee.clone(),
        })
    }

    pub fn query_pools(
        &self,
        querier: &QuerierWrapper,
        api: &dyn Api,
        contract_addr: Addr,
    ) -> StdResult<[Asset; 2]> {
        let info_0: AssetInfo = self.asset_infos[0].to_normal(api)?;
        let info_1: AssetInfo = self.asset_infos[1].to_normal(api)?;
        Ok([
            Asset {
                amount: info_0.query_pool(querier, contract_addr.clone())?,
                info: info_0,
            },
            Asset {
                amount: info_1.query_pool(querier, contract_addr)?,
                info: info_1,
            },
        ])
    }
}

pub fn pair_key(asset_infos: &[AssetInfoRaw; 2]) -> Vec<u8> {
    pair_key_from_asset_keys(asset_infos[0].as_bytes(), asset_infos[1].as_bytes())
}

pub fn pair_key_from_asset_keys(ask_asset_key: &[u8], offer_asset_key: &[u8]) -> Vec<u8> {
    // fastest way to sort in ASC order
    match ask_asset_key.le(offer_asset_key) {
        true => [ask_asset_key, offer_asset_key].concat(),
        false => [offer_asset_key, ask_asset_key].concat(),
    }
}
