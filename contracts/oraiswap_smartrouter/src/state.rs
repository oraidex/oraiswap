use cosmwasm_schema::cw_serde;

use cosmwasm_std::{Addr, StdResult, Storage};
use cw_storage_plus::{Item, Map};
use oraiswap::router::{RouterController, SwapOperation};

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub router_contract: RouterController,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const ROUTING_TABLE: Map<(&str, &str), Vec<Vec<SwapOperation>>> = Map::new("routing_table");

pub fn store_route(
    storage: &mut dyn Storage,
    key: (&str, &str),
    route: &Vec<SwapOperation>,
) -> StdResult<()> {
    match ROUTING_TABLE.may_load(storage, key)? {
        Some(mut routes) => {
            routes.push(route.to_owned());
            ROUTING_TABLE.save(storage, key, &routes)?;
        }
        None => ROUTING_TABLE.save(storage, key, &vec![route.to_owned()])?,
    };
    // reverse route to map 2 ways
    let mut reversed_route = route.to_owned();
    reversed_route.reverse();
    let reversed_key = (key.1, key.0);
    let mut reversed_ops: Vec<SwapOperation> = vec![];
    for reversed_op in reversed_route {
        reversed_ops.push(match reversed_op {
            SwapOperation::OraiSwap {
                offer_asset_info,
                ask_asset_info,
            } => SwapOperation::OraiSwap {
                offer_asset_info: ask_asset_info,
                ask_asset_info: offer_asset_info,
            },
        });
    }
    if !reversed_ops.is_empty() {
        match ROUTING_TABLE.may_load(storage, reversed_key)? {
            Some(mut routes) => {
                routes.push(reversed_ops);
                ROUTING_TABLE.save(storage, reversed_key, &routes)?;
            }
            None => ROUTING_TABLE.save(storage, reversed_key, &vec![reversed_ops])?,
        }
    }
    Ok(())
}
