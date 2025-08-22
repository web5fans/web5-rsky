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
use anyhow::bail;
use aws_sdk_s3::Config;
use futures::stream::{self, StreamExt};
use lexicon_cid::Cid;
use rocket::serde::json::Json;
use rocket::State;
use rsky_lexicon::com::atproto::web5::{
    PreDirectWritesInput, PreDirectWritesInputRefWrite, PreDirectWritesOutput,
};
use rsky_repo::types::PreparedWrite;
use std::str::FromStr;

async fn inner_pre_writes(
    body: Json<PreDirectWritesInput>,
    auth: AccessStandardIncludeChecks,
    _sequencer: &State<SharedSequencer>,
    s3_config: &State<Config>,
    db: DbConn,
    account_manager: AccountManager,
) -> Result<PreDirectWritesOutput, ApiError> {
    let tx: PreDirectWritesInput = body.into_inner();
    let PreDirectWritesInput {
        repo,
        validate,
        swap_commit,
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
        if tx.writes.len() > 200 {
            return Err(ApiError::InvalidRequest(
                "Too many writes. Max: 200".to_string(),
            ));
        }

        let writes: Vec<PreparedWrite> = stream::iter(tx.writes)
            .then(|write| async move {
                Ok::<PreparedWrite, anyhow::Error>(match write {
                    PreDirectWritesInputRefWrite::Create(write) => PreparedWrite::Create(
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
                    PreDirectWritesInputRefWrite::Update(write) => PreparedWrite::Update(
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
                    PreDirectWritesInputRefWrite::Delete(write) => {
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
            Some(swap_commit) => Some(
                Cid::from_str(&swap_commit)
                    .map_err(|_| ApiError::InvalidRequest("Swap commit convert error".to_string()))?,
            ),
            None => None,
        };

        let mut actor_store = ActorStore::new(
            did.clone(),
            S3BlobStore::new(did.clone(), s3_config.inner().clone()),
            db,
        );

        let commit = actor_store
            .generate_commit(writes.clone(), swap_commit_cid)
            .await?;

        let un_sign_bytes =
            hex::encode(serde_ipld_dagcbor::to_vec(&commit).map_err(|_| {
                ApiError::InvalidRequest("Dag-cbor encoding commit error".to_string())
            })?);

        return Ok(PreDirectWritesOutput {
            did: commit.did,
            rev: commit.rev,
            data: commit.data.to_string(),
            prev: commit.prev.map(|cid| cid.to_string()),
            version: commit.version,
            un_sign_bytes,
        });
    } else {
        Err(ApiError::InvalidRequest(format!(
            "Could not find repo: `{repo}`"
        )))
    }
}

#[tracing::instrument(skip_all)]
#[rocket::post(
    "/xrpc/com.atproto.web5.preDirectWrites",
    format = "json",
    data = "<body>"
)]
pub async fn pre_direct_writes(
    body: Json<PreDirectWritesInput>,
    auth: AccessStandardIncludeChecks,
    sequencer: &State<SharedSequencer>,
    s3_config: &State<Config>,
    db: DbConn,
    account_manager: AccountManager,
) -> Result<Json<PreDirectWritesOutput>, ApiError> {
    tracing::debug!("@LOG: debug apply_writes {body:#?}");
    match inner_pre_writes(body, auth, sequencer, s3_config, db, account_manager).await {
        Ok(res) => Ok(Json(res)),
        Err(error) => {
            tracing::error!("@LOG: ERROR: {error:?}");
            Err(error)
        }
    }
}
