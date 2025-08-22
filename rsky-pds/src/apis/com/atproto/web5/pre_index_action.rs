use crate::apis::ApiError;
use crate::plc::web5_types::{generate_challenge, get_didoc_from_chain};
use rocket::serde::json::Json;
use rsky_lexicon::com::atproto::web5::{
    PreIndexActionInput, PreIndexActionInputRef, PreIndexActionOutput, RefDeleteAccountIndex,
};

#[tracing::instrument(skip_all)]
async fn inner_pre_index_action(
    body: Json<PreIndexActionInput>,
) -> Result<PreIndexActionOutput, ApiError> {
    let PreIndexActionInput {
        did,
        ckb_addr,
        index,
    } = body.into_inner();
    let did = did.to_lowercase();
    let ckb_addr = ckb_addr.ok_or(ApiError::CkbAddrNotFound)?;

    let handle = match get_didoc_from_chain(&ckb_addr).await {
        Ok(didoc) => {
            if didoc.also_known_as.len() == 0 || !didoc.also_known_as[0].starts_with("at://") {
                return Err(ApiError::IncompatibleDidDoc);
            }
            didoc.also_known_as[0][5..].to_string()
        }
        Err(ApiError::CkbDidocCellNotFound)
            if index == PreIndexActionInputRef::DeleteAccountIndex(RefDeleteAccountIndex {}) =>
        {
            "deleteHandle".to_string()
        }
        Err(error) => return Err(error),
    };

    let domain = std::env::var("PDS_HOSTNAME").map_err(|_| ApiError::UnsupportedDomain)?;

    Ok(PreIndexActionOutput {
        did,
        handle: handle.clone(),
        message: generate_challenge(domain, ckb_addr, handle, &index)?,
    })
}

#[rocket::post(
    "/xrpc/com.atproto.web5.preIndexAction",
    format = "json",
    data = "<body>"
)]
pub async fn pre_index_action(
    body: Json<PreIndexActionInput>,
) -> Result<Json<PreIndexActionOutput>, ApiError> {
    // @TODO: Add rate limiting
    match inner_pre_index_action(body).await {
        Ok(res) => Ok(Json(res)),
        Err(error) => Err(error),
    }
}
