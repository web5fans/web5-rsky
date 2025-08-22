use crate::account_manager::AccountManager;
use crate::actor_store::aws::s3::S3BlobStore;
use crate::actor_store::ActorStore;
use crate::apis::ApiError;
use crate::db::DbConn;
use crate::plc::web5_types::statement_check;
use crate::{
    account_manager::helpers::account::{AccountStatus, AvailabilityFlags},
    plc::web5_types::{extract_timestamp, get_didoc_from_chain, timestamp_check},
};
use crate::{sequencer, SharedSequencer};
use aws_sdk_s3::Config;
use rocket::serde::json::Json;
use rocket::State;
use rsky_lexicon::com::atproto::web5::{
    IndexActionInput, IndexActionInputRef, IndexActionOutput, IndexActionOutputRefResult,
    RefCreateSessionResult, RefDeleteAccountIndex, RefDeleteAccountResult,
};
use serde_json::json;
use sha2::{Digest, Sha256};

#[tracing::instrument(skip_all)]
async fn inner_index_action(
    body: Json<IndexActionInput>,
    account_manager: AccountManager,
    sequencer: &State<SharedSequencer>,
    s3_config: &State<Config>,
    db: DbConn,
) -> Result<IndexActionOutput, ApiError> {
    let IndexActionInput {
        did,
        message,
        signing_key,
        signed_bytes,
        ckb_addr,
        index,
    } = body.into_inner();
    let did = did.to_lowercase();
    let ckb_addr = ckb_addr.ok_or(ApiError::CkbAddrNotFound)?;

    let user = account_manager
        .get_account(
            &did,
            Some(AvailabilityFlags {
                include_deactivated: Some(true),
                include_taken_down: Some(true),
            }),
        )
        .await;
    if let Ok(Some(user)) = user {
        if user.ckb_address != Some(ckb_addr.clone()) {
            return Err(ApiError::InvalidRequest(
                "Address is inconsistent with the original".to_string(),
            ));
        }

        let (did_doc, handle) = match get_didoc_from_chain(&ckb_addr).await {
            Ok(didoc) => {
                if didoc.also_known_as.len() == 0 || !didoc.also_known_as[0].starts_with("at://") {
                    return Err(ApiError::IncompatibleDidDoc);
                }
                let handle = didoc.also_known_as[0][5..].to_string();
                if user.handle.ok_or(ApiError::InvalidHandle)? != handle {
                    return Err(ApiError::InvalidHandle);
                }
                let doc_keys: Vec<String> = didoc.verification_methods.values().cloned().collect();
                if !doc_keys.contains(&signing_key) {
                    return Err(ApiError::InvalidRequest(
                        "Signing key is inconsistent with the did doc".to_string(),
                    ));
                }
                (Some(json!(didoc)), handle)
            }
            Err(ApiError::CkbDidocCellNotFound) => (None, "deleteHandle".to_string()),
            Err(error) => return Err(error),
        };

        if !timestamp_check(extract_timestamp(&message)?)? {
            return Err(ApiError::InvalidRequest("Sign message timeout".to_string()));
        }
        let hash = Sha256::digest(&message);
        let sig = if signed_bytes.starts_with("0x") || signed_bytes.starts_with("0X") {
            &signed_bytes[2..]
        } else {
            &signed_bytes
        };
        if !statement_check(&message, &index)? {
            return Err(ApiError::InvalidRequest(
                "Message statement check error".to_string(),
            ));
        }
        if !rsky_crypto::verify::verify_signature(
            &signing_key,
            hash.as_ref(),
            &hex::decode(sig).map_err(|error| {
                ApiError::InvalidRequest(format!("Signature decode error {error}"))
            })?,
            None,
        )? {
            tracing::error!("web5 create session verify signature failed");
            return Err(ApiError::RuntimeError);
        }
        match index {
            IndexActionInputRef::CreateSessionIndex(_) => {
                let (access_jwt, refresh_jwt);
                match account_manager.create_session(user.did.clone(), None).await {
                    Ok(res) => {
                        (access_jwt, refresh_jwt) = res;
                    }
                    Err(e) => {
                        tracing::error!("{e:?}");
                        return Err(ApiError::RuntimeError);
                    }
                }
                let ref_csr = RefCreateSessionResult {
                    did,
                    did_doc,
                    handle,
                    email: user.email,
                    email_confirmed: Some(user.email_confirmed_at.is_some()),
                    access_jwt,
                    refresh_jwt,
                };
                Ok(IndexActionOutput {
                    result: IndexActionOutputRefResult::CreateSessionResult(ref_csr),
                })
            }
            IndexActionInputRef::DeleteAccountIndex(_) => {
                let mut actor_store = ActorStore::new(
                    did.clone(),
                    S3BlobStore::new(did.clone(), s3_config.inner().clone()),
                    db,
                );
                actor_store.destroy().await?;
                account_manager.delete_account(&did).await?;
                let mut lock = sequencer.sequencer.write().await;
                let account_seq = lock
                    .sequence_account_evt(did.clone(), AccountStatus::Deleted)
                    .await?;
                sequencer::delete_all_for_user(&did, Some(vec![account_seq])).await?;
                Ok(IndexActionOutput {
                    result: IndexActionOutputRefResult::DeleteAccountResult(
                        RefDeleteAccountResult {},
                    ),
                })
            }
        }
    } else {
        Err(ApiError::InvalidLogin)
    }
}

#[rocket::post("/xrpc/com.atproto.web5.indexAction", format = "json", data = "<body>")]
pub async fn index_action(
    body: Json<IndexActionInput>,
    account_manager: AccountManager,
    sequencer: &State<SharedSequencer>,
    s3_config: &State<Config>,
    db: DbConn,
) -> Result<Json<IndexActionOutput>, ApiError> {
    // @TODO: Add rate limiting
    match inner_index_action(body, account_manager, sequencer, s3_config, db).await {
        Ok(res) => Ok(Json(res)),
        Err(error) => Err(error),
    }
}
