use serde_json::Value;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreCreateAccountInput {
    pub handle: String,
    pub did: String,
    pub signing_key: Option<String>,
    pub invite_code: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreCreateAccountOutput {
    pub did: String,
    pub rev: String,
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
    pub version: u8,
    pub un_sign_bytes: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccountInput {
    pub handle: String,
    pub signing_key: String,
    pub password: String,
    pub root: SignedRoot,
    pub ckb_addr: String,
    pub invite_code: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateAccountOutput {
    pub handle: String,
    pub did: String,
    #[serde(rename = "didDoc", skip_serializing_if = "Option::is_none")]
    pub did_doc: Option<Value>,
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
}

/// Pre apply writes output
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "com.atproto.web5.createAccount#signedRoot")]
pub struct SignedRoot {
    pub did: String,
    pub rev: String,
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
    pub version: u8,
    pub signed_bytes: String,
}

/// Pre apply a batch transaction of repository creates, updates, and deletes.
/// Requires auth, implemented by PDS.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PreDirectWritesInput {
    /// The handle or DID of the repo (aka, current account).
    pub repo: String,
    /// Can be set to 'false' to skip Lexicon schema validation of record data, for all operations.
    pub validate: Option<bool>,
    /// The Record Key.
    pub writes: Vec<PreDirectWritesInputRefWrite>,
    /// Compare and swap with the previous commit by CID.
    #[serde(rename = "swapCommit", skip_serializing_if = "Option::is_none")]
    pub swap_commit: Option<String>,
}

/// Direct apply a batch transaction of repository creates, updates, and deletes.
/// Requires auth, implemented by PDS.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectWritesInput {
    /// The handle or DID of the repo (aka, current account).
    pub repo: String,
    /// Can be set to 'false' to skip Lexicon schema validation of record data, for all operations.
    pub validate: Option<bool>,
    /// The Record Key.
    pub writes: Vec<DirectWritesInputRefWrite>,
    /// Compare and swap with the previous commit by CID.
    #[serde(rename = "swapCommit", skip_serializing_if = "Option::is_none")]
    pub swap_commit: Option<String>,
    /// Signing bytes on PreDirectWritesInput return
    pub signing_key: String,
    pub ckb_addr: Option<String>,
    pub root: SignedRoot,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "$type")]
pub enum PreDirectWritesInputRefWrite {
    #[serde(rename = "com.atproto.web5.preDirectWrites#create")]
    Create(RefWriteCreate),
    #[serde(rename = "com.atproto.web5.preDirectWrites#update")]
    Update(RefWriteUpdate),
    #[serde(rename = "com.atproto.web5.preDirectWrites#delete")]
    Delete(RefWriteDelete),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "$type")]
pub enum DirectWritesInputRefWrite {
    #[serde(rename = "com.atproto.web5.directWrites#create")]
    Create(RefWriteCreate),
    #[serde(rename = "com.atproto.web5.directWrites#update")]
    Update(RefWriteUpdate),
    #[serde(rename = "com.atproto.web5.directWrites#delete")]
    Delete(RefWriteDelete),
}

/// Operation which creates a new record.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RefWriteCreate {
    pub collection: String,
    pub rkey: Option<String>,
    pub value: Value,
}

/// Operation which updates an existing record.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RefWriteUpdate {
    pub collection: String,
    pub rkey: String,
    pub value: Value,
}

/// Operation which deletes an existing record.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RefWriteDelete {
    pub collection: String,
    pub rkey: String,
}

/// Pre apply writes output
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreDirectWritesOutput {
    pub did: String,
    pub rev: String,
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
    pub version: u8,
    pub un_sign_bytes: String,
}

/// Create an authentication session.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    /// Handle or other identifier supported by the server for the authenticating user.
    pub identifier: String,
    pub password: String,
    pub ckb_addr: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateSessionOutput {
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
    pub handle: String,
    pub did: String,
    #[serde(rename = "didDoc", skip_serializing_if = "Option::is_none")]
    pub did_doc: Option<String>,
    pub email: Option<String>,
    #[serde(rename = "emailConfirmed", skip_serializing_if = "Option::is_none")]
    pub email_confirmed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectWritesOutput {
    pub commit: Option<CommitMeta>,
    pub results: Option<Vec<DirectWritesOutputRefWrite>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "$type")]
pub enum DirectWritesOutputRefWrite {
    #[serde(rename = "com.atproto.web5.directWrites#createResult")]
    Create(RefWriteCreateResult),
    #[serde(rename = "com.atproto.web5.directWrites#updateResult")]
    Update(RefWriteUpdateResult),
    #[serde(rename = "com.atproto.web5.directWrites#deleteResult")]
    Delete(RefWriteDeleteResult),
}

/// Operation which creates a new record.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefWriteCreateResult {
    pub uri: String,
    pub cid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
}

/// Operation which updates an existing record.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefWriteUpdateResult {
    pub uri: String,
    pub cid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
}

/// Operation which deletes an existing record.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RefWriteDeleteResult {}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename = "com.atproto.repo.defs#commitMeta")]
pub struct CommitMeta {
    pub cid: String,
    pub rev: String,
}
