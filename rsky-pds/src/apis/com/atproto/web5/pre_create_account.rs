use crate::account_manager::AccountManager;
use crate::actor_store::aws::s3::S3BlobStore;
use crate::actor_store::ActorStore;
use crate::apis::ApiError;
use crate::auth_verifier::UserDidAuthOptional;
use crate::config::ServerConfig;
use crate::db::DbConn;
use crate::handle::{normalize_and_validate_handle, HandleValidationContext, HandleValidationOpts};
use crate::SharedIdResolver;
use crate::SharedSequencer;
use aws_sdk_s3::Config;
use rocket::serde::json::Json;
use rocket::State;
use rsky_lexicon::com::atproto::web5::{PreCreateAccountInput, PreCreateAccountOutput};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransformedWeb5CreateAccountInput {
    pub handle: String,
    pub did: String,
    pub invite_code: Option<String>,
}

//TODO: Potential for taking advantage of async better
#[tracing::instrument(skip_all)]
#[rocket::post(
    "/xrpc/com.atproto.web5.preCreateAccount",
    format = "json",
    data = "<body>"
)]
pub async fn pre_create_account(
    body: Json<PreCreateAccountInput>,
    _auth: UserDidAuthOptional,
    _sequencer: &State<SharedSequencer>,
    s3_config: &State<Config>,
    cfg: &State<ServerConfig>,
    id_resolver: &State<SharedIdResolver>,
    account_manager: AccountManager,
    db: DbConn,
) -> Result<Json<PreCreateAccountOutput>, ApiError> {
    tracing::info!("PreCreating new user account");
    // @TODO: Evaluate if we need to validate for entryway PDS
    let TransformedWeb5CreateAccountInput {
        handle: _,
        did,
        invite_code: _,
    } = match validate_inputs_for_local_pds(cfg, id_resolver, body.into_inner(), &account_manager)
        .await
    {
        Ok(input) => input,
        Err(error) => {
            tracing::error!("Failed to validate inputs\n{:?}", error);
            return Err(ApiError::RuntimeError);
        }
    };

    // Create new actor repo TODO: Proper rollback
    let mut actor_store = ActorStore::new(
        did.clone(),
        S3BlobStore::new(did.clone(), s3_config.inner().clone()),
        db,
    );
    match actor_store.pre_create_repo(Vec::new()).await {
        Ok(commit) => Ok(Json(commit)),
        Err(error) => {
            tracing::error!("Failed to create repo\n{:?}", error);
            actor_store.destroy().await?;
            Err(ApiError::RuntimeError)
        }
    }
}

/// Validates Create Account Parameters and builds PLC Operation if needed
pub async fn validate_inputs_for_local_pds(
    cfg: &State<ServerConfig>,
    id_resolver: &State<SharedIdResolver>,
    input: PreCreateAccountInput,
    account_manager: &AccountManager,
) -> Result<TransformedWeb5CreateAccountInput, ApiError> {
    //Invite Code Validation
    let invite_code = if cfg.invites.required && input.invite_code.is_none() {
        return Err(ApiError::InvalidInviteCode);
    } else {
        input.invite_code
    };

    // Normalize and Ensure Valid Handle
    let opts = HandleValidationOpts {
        handle: input.handle,
        did: Some(input.did.clone()),
        allow_reserved: None,
    };
    let validation_ctx = HandleValidationContext {
        server_config: cfg,
        id_resolver,
    };
    let handle = normalize_and_validate_handle(opts, validation_ctx).await?;
    if !super::validate_handle(&handle) {
        return Err(ApiError::InvalidHandle);
    };

    // Check Handle is still available
    let handle_accnt = account_manager.get_account(&handle, None).await?;
    if handle_accnt.is_some() {
        return Err(ApiError::HandleNotAvailable);
    }

    Ok(TransformedWeb5CreateAccountInput {
        handle,
        did: input.did,
        invite_code,
    })
}
