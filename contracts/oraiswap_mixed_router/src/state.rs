use cosmwasm_schema::cw_serde;

use cosmwasm_std::CanonicalAddr;
use cw_storage_plus::Item;

#[cw_serde]
pub struct Config {
    pub factory_addr: CanonicalAddr,
    pub factory_addr_v2: CanonicalAddr,
    pub oraiswap_v3: CanonicalAddr,
    pub owner: CanonicalAddr,
}

// put the length bytes at the first for compatibility with legacy singleton store
pub const CONFIG: Item<Config> = Item::new("\u{0}\u{6}config");
