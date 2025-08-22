use crate::account_manager::helpers::account::AccountStatus;
use crate::account_manager::{AccountManager, CreateAccountOpts};
use crate::actor_store::aws::s3::S3BlobStore;
use crate::actor_store::ActorStore;
use crate::apis::ApiError;
use crate::auth_verifier::UserDidAuthOptional;
use crate::config::ServerConfig;
use crate::db::DbConn;
use crate::plc::web5_types::{generate_random_string, get_didoc_from_chain};
use crate::sequencer::events::sync_evt_data_from_commit;
use crate::SharedSequencer;
use aws_sdk_s3::Config;
use rocket::serde::json::Json;
use rocket::State;
use rsky_lexicon::com::atproto::web5::{CreateAccountInput, CreateAccountOutput};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransformedWeb5CreateAccountInput {
    pub handle: String,
    pub did: String,
    pub password: String,
    pub invite_code: Option<String>,
}

//TODO: Potential for taking advantage of async better
#[tracing::instrument(skip_all)]
#[rocket::post(
    "/xrpc/com.atproto.web5.createAccount",
    format = "json",
    data = "<body>"
)]
pub async fn create_account(
    body: Json<CreateAccountInput>,
    _auth: UserDidAuthOptional,
    sequencer: &State<SharedSequencer>,
    s3_config: &State<Config>,
    _cfg: &State<ServerConfig>,
    account_manager: AccountManager,
    db: DbConn,
) -> Result<Json<CreateAccountOutput>, ApiError> {
    tracing::info!("Creating new user account");
    // @TODO: Evaluate if we need to validate for entryway PDS
    let input: CreateAccountInput = body.into_inner();
    let did = input.root.did.clone();
    let handle = input.handle.clone();

    match get_didoc_from_chain(&input.ckb_addr).await {
        Ok(_) => {
            return Err(ApiError::InvalidCkbError(format!(
                "Already apply did, please change address."
            )))
        }
        Err(ApiError::CkbDidocCellNotFound) => {},
        Err(error) => return Err(error),
    }

    // Create new actor repo TODO: Proper rollback
    let mut actor_store = ActorStore::new(
        did.clone(),
        S3BlobStore::new(did.clone(), s3_config.inner().clone()),
        db,
    );
    let commit = match actor_store
        .web5_create_repo(input.root, input.signing_key, Vec::new())
        .await
    {
        Ok(commit) => commit,
        Err(error) => {
            tracing::error!("Failed to create repo\n{:?}", error);
            actor_store.destroy().await?;
            return Err(ApiError::RuntimeError);
        }
    };

    // Create Account
    let (access_jwt, refresh_jwt);
    match account_manager
        .create_account(CreateAccountOpts {
            did: did.clone(),
            handle: handle.clone(),
            email: Some(format!("web5Mock{}@web5.com", generate_random_string(10))),
            password: Some(generate_random_string(16)),
            repo_cid: commit.commit_data.cid,
            repo_rev: commit.commit_data.rev.clone(),
            invite_code: input.invite_code,
            deactivated: Some(false),
            ckb_addr: Some(input.ckb_addr),
        })
        .await
    {
        Ok(res) => {
            (access_jwt, refresh_jwt) = res;
        }
        Err(error) => {
            tracing::error!("Error creating account\n{error}");
            actor_store.destroy().await?;
            return Err(ApiError::RuntimeError);
        }
    }

    let mut lock = sequencer.sequencer.write().await;
    match lock
        .sequence_identity_evt(did.clone(), Some(handle.clone()))
        .await
    {
        Ok(_) => {
            tracing::debug!("Sequenece identity event succeeded");
        }
        Err(error) => {
            tracing::error!("Sequence Identity Event failed\n{error}");
            return Err(ApiError::RuntimeError);
        }
    }
    match lock
        .sequence_account_evt(did.clone(), AccountStatus::Active)
        .await
    {
        Ok(_) => {
            tracing::debug!("Sequence account event succeeded");
        }
        Err(error) => {
            tracing::error!("Sequence Account Event failed\n{error}");
            return Err(ApiError::RuntimeError);
        }
    }
    match lock.sequence_commit(did.clone(), commit.clone()).await {
        Ok(_) => {
            tracing::debug!("Sequence commit succeeded");
        }
        Err(error) => {
            tracing::error!("Sequence Commit failed\n{error}");
            return Err(ApiError::RuntimeError);
        }
    }
    match lock
        .sequence_sync_evt(
            did.clone(),
            sync_evt_data_from_commit(commit.clone()).await?,
        )
        .await
    {
        Ok(_) => {
            tracing::debug!("Sequence sync event data from commit succeeded");
        }
        Err(error) => {
            tracing::error!("Sequence sync event data from commit failed\n{error}");
            return Err(ApiError::RuntimeError);
        }
    }
    match account_manager
        .update_repo_root(did.clone(), commit.commit_data.cid, commit.commit_data.rev)
        .await
    {
        Ok(_) => {
            tracing::debug!("Successfully updated repo root");
        }
        Err(error) => {
            tracing::error!("Update Repo Root failed\n{error}");
            return Err(ApiError::RuntimeError);
        }
    }

    // let converted_did_doc;
    // match did_doc {
    //     None => converted_did_doc = None,
    //     Some(did_doc) => match serde_json::to_value(did_doc) {
    //         Ok(res) => converted_did_doc = Some(res),
    //         Err(error) => {
    //             tracing::error!("Did Doc failed conversion\n{error}");
    //             return Err(ApiError::RuntimeError);
    //         }
    //     },
    // }

    Ok(Json(CreateAccountOutput {
        access_jwt,
        refresh_jwt,
        handle,
        did,
        did_doc: None,
    }))
}
