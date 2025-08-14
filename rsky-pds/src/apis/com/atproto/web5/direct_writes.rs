use crate::account_manager::helpers::account::AvailabilityFlags;
use crate::account_manager::AccountManager;
use crate::actor_store::aws::s3::S3BlobStore;
use crate::actor_store::ActorStore;
use crate::apis::ApiError;
use crate::auth_verifier::AccessStandardIncludeChecks;
use crate::db::DbConn;
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
) -> Result<DirectWritesOutput> {
    let tx: DirectWritesInput = body.into_inner();
    let DirectWritesInput {
        repo,
        validate,
        swap_commit,
        root,
        ..
    } = tx;
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
        if account.deactivated_at.is_some() {
            bail!("Account is deactivated")
        }
        let did = account.did;
        if did != auth.access.credentials.unwrap().did.unwrap() {
            bail!("AuthRequiredError")
        }
        let did: &String = &did;
        if tx.writes.len() > 200 {
            bail!("Too many writes. Max: 200")
        }

        let writes: Vec<PreparedWrite> = stream::iter(tx.writes)
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

        let swap_commit_cid = match swap_commit {
            Some(swap_commit) => Some(Cid::from_str(&swap_commit)?),
            None => None,
        };

        let mut actor_store = ActorStore::new(
            did.clone(),
            S3BlobStore::new(did.clone(), s3_config.inner().clone()),
            db,
        );

        let commit = actor_store
            .verify_writes(writes.clone(), swap_commit_cid, tx.signing_key, root)
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
            results: Some(writes
                .iter()
                .map(|write| write_to_output_result(write, validate))
                .collect()),
        })
    } else {
        bail!("Could not find repo: `{repo}`")
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
            tracing::error!("@LOG: ERROR: {error}");
            Err(ApiError::RuntimeError)
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
