use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, Addr};
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};

use cw20::Cw20ExecuteMsg;
use cw20_base::ContractError;
use cw20_base::{
    contract::{
        execute as cw20_execute, instantiate as cw20_instantiate, migrate as cw20_migrate,
        query as cw20_query,
    },
    msg::{InstantiateMsg, MigrateMsg as Cw20MigrateMsg, QueryMsg},
};
use cw_storage_plus::Map;

pub const BLACKLIST: Map<Addr, bool> = Map::new("black_list");

#[cw_serde]
pub struct MigrateMsg {
    pub addr: Vec<Addr>,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    cw20_instantiate(deps, env, info, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Cw20ExecuteMsg,
) -> Result<Response, ContractError> {
    let is_black_list = BLACKLIST
        .may_load(deps.storage, info.sender.clone())
        .unwrap_or_default();
    if let Some(is_black_list) = is_black_list {
        if is_black_list {
            return Err(ContractError::Unauthorized {});
        }
    }

    cw20_execute(deps, env, info, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    cw20_query(deps, env, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    for addr in msg.addr {
        BLACKLIST.save(deps.storage, addr, &true)?;
    }
    let cw20_migrate_msg = Cw20MigrateMsg {};
    cw20_migrate(deps, env, cw20_migrate_msg)
}

#[test]
pub fn test() {
    let contract = Box::new(oraiswap::create_entry_points_testing!(crate));
    let mut app = oraiswap::testing::MockApp::new(&[]);
    let code_id = app.upload(contract);
    println!("contract code id {}", code_id);
}
