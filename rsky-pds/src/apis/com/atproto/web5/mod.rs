use crate::{apis::ApiError, lexicon::lexicons::Add};
use ckb_sdk::Address;
use std::{env, str::FromStr};

pub fn validate_handle(handle: &str) -> bool {
    let suffix: String = env::var("PDS_HOSTNAME").unwrap_or("localhost".to_owned());
    let s_slice: &str = &suffix[..]; // take a full slice of the string
    handle.ends_with(s_slice)
    // Need to check suffix here and need to make sure handle doesn't include "." after trumming it
}

// pub async fn get_didoc_from_chain(ckb_addr: String) -> Result<DidDocument, ApiError> {
//    let addr = Address::from_str(&ckb_addr).map_err(|_| ApiError::InvalidCkbAddr)?;
//    let address_hash = addr;
//    let query_url = format!("http://testnet-api.explorer.nervos.org/api/v2/scripts/referring_cells?code_hash=0x510150477b10d6ab551a509b71265f3164e9fd4137fcb5a4322f49f03092c7c5&hash_type=type&sort=created_time.asc&address_hash={}&restrict=false&page=1&page_size=1", address_hash);
//
//    return Err(ApiError::InvalidCkbAddr);
// }

pub mod create_account;
pub mod create_session;
pub mod direct_writes;
pub mod pre_create_account;
pub mod pre_direct_writes;
