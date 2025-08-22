use crate::account_manager::helpers::account::AvailabilityFlags;
use crate::account_manager::AccountManager;
use crate::actor_store::aws::s3::S3BlobStore;
use crate::actor_store::ActorStore;
use crate::apis::ApiError;
use crate::auth_verifier::AccessStandardIncludeChecks;
use crate::db::DbConn;
use crate::plc::web5_types::get_didoc_from_chain;
use crate::repo::prepare::{
    prepare_create, prepare_delete, prepare_update, PrepareCreateOpts, PrepareDeleteOpts,
    PrepareUpdateOpts,
};
use crate::SharedSequencer;
use anyhow::{bail, Result};
use aws_sdk_s3::Config;
use futures::stream::{self, StreamExt};
use lexicon_cid::Cid;
use rocket::serde::json::Json;
use rocket::State;
use rsky_lexicon::com::atproto::web5::{
    CommitMeta, DirectWritesInput, DirectWritesInputRefWrite, DirectWritesOutput,
    DirectWritesOutputRefWrite, RefWriteCreateResult, RefWriteDeleteResult, RefWriteUpdateResult,
};
use rsky_repo::types::PreparedWrite;
use std::str::FromStr;

async fn inner_direct_writes(
    body: Json<DirectWritesInput>,
    auth: AccessStandardIncludeChecks,
    sequencer: &State<SharedSequencer>,
    s3_config: &State<Config>,
    db: DbConn,
    account_manager: AccountManager,
) -> Result<DirectWritesOutput, ApiError> {
    let tx: DirectWritesInput = body.into_inner();
    let DirectWritesInput {
        repo,
        validate,
        swap_commit,
        writes,
        signing_key,
        ckb_addr,
        root,
    } = tx;
    let ckb_addr = ckb_addr.ok_or(ApiError::CkbAddrNotFound)?;
    let account = account_manager
        .get_account(
            &repo,
            Some(AvailabilityFlags {
                include_deactivated: Some(true),
                include_taken_down: None,
            }),
        )
        .await?;

    if let Some(account) = account {
        if account.ckb_address != Some(ckb_addr.clone()) {
            return Err(ApiError::InvalidRequest(
                "Address is inconsistent with the original".to_string(),
            ));
        }

        match get_didoc_from_chain(&ckb_addr).await {
            Ok(didoc) => {
                if didoc.also_known_as.len() == 0 || !didoc.also_known_as[0].starts_with("at://") {
                    return Err(ApiError::IncompatibleDidDoc);
                }
                let handle = didoc.also_known_as[0][5..].to_string();
                if account.handle.ok_or(ApiError::InvalidHandle)? != handle {
                    return Err(ApiError::InvalidHandle);
                }
                let doc_keys: Vec<String> = didoc.verification_methods.values().cloned().collect();
                if !doc_keys.contains(&signing_key) {
                    return Err(ApiError::InvalidRequest(
                        "Signing key is inconsistent with the did doc".to_string(),
                    ));
                }
            }
            Err(error) => return Err(error),
        };

        if account.deactivated_at.is_some() {
            return Err(ApiError::InvalidRequest(
                "Account is deactivated".to_string(),
            ));
        }
        let did = account.did;
        if did
            != auth
                .access
                .credentials
                .ok_or(ApiError::AuthRequiredError("".to_string()))?
                .did
                .ok_or(ApiError::InvalidRequest(
                    "Auth credentials require did ".to_string(),
                ))?
        {
            return Err(ApiError::AuthRequiredError(
                "Did is inconsistent with origin".to_string(),
            ));
        }
        let did: &String = &did;
        if writes.len() > 200 {
            return Err(ApiError::InvalidRequest(
                "Too many writes. Max: 200".to_string(),
            ));
        }

        let writes: Vec<PreparedWrite> = stream::iter(writes)
            .then(|write| async move {
                Ok::<PreparedWrite, anyhow::Error>(match write {
                    DirectWritesInputRefWrite::Create(write) => PreparedWrite::Create(
                        prepare_create(PrepareCreateOpts {
                            did: did.clone(),
                            collection: write.collection,
                            rkey: write.rkey,
                            swap_cid: None,
                            record: serde_json::from_value(write.value)?,
                            validate,
                        })
                        .await?,
                    ),
                    DirectWritesInputRefWrite::Update(write) => PreparedWrite::Update(
                        prepare_update(PrepareUpdateOpts {
                            did: did.clone(),
                            collection: write.collection,
                            rkey: write.rkey,
                            swap_cid: None,
                            record: serde_json::from_value(write.value)?,
                            validate,
                        })
                        .await?,
                    ),
                    DirectWritesInputRefWrite::Delete(write) => {
                        PreparedWrite::Delete(prepare_delete(PrepareDeleteOpts {
                            did: did.clone(),
                            collection: write.collection,
                            rkey: write.rkey,
                            swap_cid: None,
                        })?)
                    }
                })
            })
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<PreparedWrite>, _>>()?;

        let swap_commit_cid =
            match swap_commit {
                Some(swap_commit) => Some(Cid::from_str(&swap_commit).map_err(|_| {
                    ApiError::InvalidRequest("Swap commit convert error".to_string())
                })?),
                None => None,
            };

        let mut actor_store = ActorStore::new(
            did.clone(),
            S3BlobStore::new(did.clone(), s3_config.inner().clone()),
            db,
        );

        let commit = actor_store
            .verify_writes(writes.clone(), swap_commit_cid, signing_key, root)
            .await?;

        let mut lock = sequencer.sequencer.write().await;
        lock.sequence_commit(did.clone(), commit.clone()).await?;
        account_manager
            .update_repo_root(
                did.to_string(),
                commit.commit_data.cid.clone(),
                commit.commit_data.rev.clone(),
            )
            .await?;

        Ok(DirectWritesOutput {
            commit: Some(CommitMeta {
                cid: commit.commit_data.cid.to_string(),
                rev: commit.commit_data.rev,
            }),
            results: Some(
                writes
                    .iter()
                    .map(|write| write_to_output_result(write, validate))
                    .collect(),
            ),
        })
    } else {
        Err(ApiError::InvalidRequest(format!(
            "Could not find repo: `{repo}`"
        )))
    }
}

#[tracing::instrument(skip_all)]
#[rocket::post(
    "/xrpc/com.atproto.web5.directWrites",
    format = "json",
    data = "<body>"
)]
pub async fn direct_writes(
    body: Json<DirectWritesInput>,
    auth: AccessStandardIncludeChecks,
    sequencer: &State<SharedSequencer>,
    s3_config: &State<Config>,
    db: DbConn,
    account_manager: AccountManager,
) -> Result<Json<DirectWritesOutput>, ApiError> {
    tracing::debug!("@LOG: debug direct_writes {body:#?}");
    match inner_direct_writes(body, auth, sequencer, s3_config, db, account_manager).await {
        Ok(output) => Ok(Json(output)),
        Err(error) => {
            tracing::error!("@LOG: ERROR: {error:?}");
            Err(error)
        }
    }
}

pub fn write_to_output_result(
    write: &PreparedWrite,
    validation: Option<bool>,
) -> DirectWritesOutputRefWrite {
    let validation_status = if validation == Some(true) {
        Some("valid".to_string())
    } else {
        None
    };
    match write {
        PreparedWrite::Create(inner) => DirectWritesOutputRefWrite::Create(RefWriteCreateResult {
            cid: inner.cid.to_string(),
            uri: inner.uri.clone(),
            validation_status,
        }),
        PreparedWrite::Update(inner) => DirectWritesOutputRefWrite::Update(RefWriteUpdateResult {
            cid: inner.cid.to_string(),
            uri: inner.uri.clone(),
            validation_status,
        }),
        PreparedWrite::Delete(_) => DirectWritesOutputRefWrite::Delete(RefWriteDeleteResult {}),
    }
}
