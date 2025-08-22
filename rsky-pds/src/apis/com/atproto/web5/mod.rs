use std::env;

pub fn validate_handle(handle: &str) -> bool {
    let suffix: String = env::var("PDS_HOSTNAME").unwrap_or("localhost".to_owned());
    let s_slice: &str = &suffix[..]; // take a full slice of the string
    handle.ends_with(s_slice)
    // Need to check suffix here and need to make sure handle doesn't include "." after trumming it
}

pub mod create_account;
pub mod index_action;
pub mod direct_writes;
pub mod pre_create_account;
pub mod pre_direct_writes;
pub mod upload_blob;
pub mod pre_index_action;
