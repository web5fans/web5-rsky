use crate::apis::ApiError;
use crate::plc::cell_data::{DidWeb5Data, DidWeb5DataUnion};
use anyhow::{bail, Result};
use ckb_jsonrpc_types::{OutPoint, Uint32};
use ckb_sdk::{Address, CkbRpcAsyncClient};
use ckb_types::{packed::Script, H256};
use molecule::prelude::Entity;
use rand::{distributions::Alphanumeric, Rng};
use rsky_lexicon::com::atproto::web5::{IndexActionInputRef, PreIndexActionInputRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::BTreeMap, str::FromStr};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Service {
    #[serde(rename = "type")]
    pub r#type: String,
    pub endpoint: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Web5DocumentData {
    #[serde(rename = "verificationMethods")]
    pub verification_methods: BTreeMap<String, String>,
    #[serde(rename = "alsoKnownAs")]
    pub also_known_as: Vec<String>,
    pub services: BTreeMap<String, Service>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateOpV1 {
    #[serde(rename = "type")]
    pub r#type: String, // string literal `create`
    #[serde(rename = "signingKey")]
    pub signing_key: String,
    #[serde(rename = "recoveryKey")]
    pub recovery_key: String,
    pub handle: String,
    pub service: String,
    pub prev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Operation {
    #[serde(rename = "type")]
    pub r#type: String, // string literal `plc_operation`
    #[serde(rename = "verificationMethods")]
    pub verification_methods: BTreeMap<String, String>,
    #[serde(rename = "alsoKnownAs")]
    pub also_known_as: Vec<String>,
    pub services: BTreeMap<String, Service>,
    // Omit<t.UnsignedOperation, 'prev'>
    pub prev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Tombstone {
    #[serde(rename = "type")]
    pub r#type: String, // string literal `plc_tombstone`
    pub prev: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)] // Needs to be signed, so we don't want an additional tag
pub enum CompatibleOpOrTombstone {
    CreateOpV1(CreateOpV1),
    Operation(Operation),
    Tombstone(Tombstone),
}

impl CompatibleOpOrTombstone {
    pub fn set_sig(&mut self, sig: String) {
        match self {
            Self::CreateOpV1(create) => create.sig = Some(sig),
            Self::Operation(op) => op.sig = Some(sig),
            Self::Tombstone(tombstone) => tombstone.sig = Some(sig),
        }
    }

    pub fn get_sig(&mut self) -> &Option<String> {
        match self {
            Self::CreateOpV1(create) => &create.sig,
            Self::Operation(op) => &op.sig,
            Self::Tombstone(tombstone) => &tombstone.sig,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)] // will be posted to API so needs to not be tagged
pub enum CompatibleOp {
    CreateOpV1(CreateOpV1),
    Operation(Operation),
}

impl CompatibleOp {
    pub fn set_sig(&mut self, sig: String) {
        match self {
            Self::CreateOpV1(create) => create.sig = Some(sig),
            Self::Operation(op) => op.sig = Some(sig),
        }
    }

    pub fn get_sig(&mut self) -> &Option<String> {
        match self {
            Self::CreateOpV1(create) => &create.sig,
            Self::Operation(op) => &op.sig,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)] // will be posted to API so needs to not be tagged
pub enum OpOrTombstone {
    Operation(Operation),
    Tombstone(Tombstone),
}

impl OpOrTombstone {
    pub fn set_sig(&mut self, sig: String) {
        match self {
            Self::Operation(op) => op.sig = Some(sig),
            Self::Tombstone(tombstone) => tombstone.sig = Some(sig),
        }
    }

    pub fn get_sig(&mut self) -> &Option<String> {
        match self {
            Self::Operation(op) => &op.sig,
            Self::Tombstone(tombstone) => &tombstone.sig,
        }
    }
}

pub async fn get_didoc_from_chain(ckb_addr: &str) -> Result<Web5DocumentData, ApiError> {
    let addr = Address::from_str(ckb_addr)
        .map_err(|_| ApiError::InvalidCkbError(format!("Address format invalid")))?;
    let script: Script = (&addr).into();
    let address_hash = "0x".to_string() + &hex::encode(script.calc_script_hash().raw_data());
    let query_url = format!("http://testnet-api.explorer.nervos.org/api/v2/scripts/referring_cells?code_hash=0x510150477b10d6ab551a509b71265f3164e9fd4137fcb5a4322f49f03092c7c5&hash_type=type&sort=created_time.asc&address_hash={}&restrict=false&page=1&page_size=1", address_hash);
    let client = reqwest::Client::new();

    let response = client
        .get(query_url)
        .send()
        .await
        .map_err(|_| ApiError::InvalidCkbError(format!("CKB Testnet")))?;
    let data = response
        .text()
        .await
        .map_err(|_| ApiError::InvalidCkbError(format!("CKB Testnet Response")))?;
    if data.len() == 0 {
        return Err(ApiError::CkbAddrNoCell);
    }
    let json: Value = serde_json::from_str(&data)
        .map_err(|_| ApiError::InvalidCkbError(format!("CKB Testnet Response Convert")))?;

    let referring_cells = json["data"]
        .as_object()
        .ok_or(ApiError::InvalidCkbError(format!(
            "CKB Testnet Response Convert data"
        )))?["referring_cells"]
        .as_array()
        .ok_or(ApiError::InvalidCkbError(format!(
            "CKB Testnet Response Convert referring_cells"
        )))?;

    if referring_cells.len() != 0 {
        let tx_hash_str = referring_cells[0]["tx_hash"].as_str().ok_or({
            ApiError::InvalidCkbError(format!("CKB Testnet Response Convert tx_hash"))
        })?;
        let cell_index = referring_cells[0]["cell_index"].as_u64().ok_or({
            ApiError::InvalidCkbError(format!("CKB Testnet Response Convert cell_index"))
        })? as u32;

        let client = CkbRpcAsyncClient::new("https://testnet.ckb.dev/");
        let tx_hash = H256::from_str(&tx_hash_str[2..])
            .map_err(|_| ApiError::InvalidCkbError(format!("CKB Testnet Response Convert Hash")))?;
        let index = Uint32::from(cell_index);
        let cell = client
            .get_live_cell(OutPoint { tx_hash, index }, true)
            .await
            .map_err(|_| ApiError::InvalidCkbError(format!("CKB get_live_cell")))?;
        if let Some(cell) = cell.cell {
            if let Some(cell_data) = cell.data {
                let bytes = cell_data.content.as_bytes();
                let did_data = DidWeb5Data::from_slice(bytes).map_err(|_| {
                    ApiError::InvalidCkbError("DidWeb5Data convert failed".to_string())
                })?;
                let DidWeb5DataUnion::DidWeb5DataV1(did_data_v1) = did_data.to_enum();
                let did_doc = did_data_v1.document();
                Ok(
                    serde_ipld_dagcbor::from_slice(&did_doc.raw_data()).map_err(|_| {
                        ApiError::InvalidCkbError(
                            "Web5DocumentData dog cbor decode failed".to_string(),
                        )
                    })?,
                )
            } else {
                return Err(ApiError::InvalidCkbError("Cell data not found".to_string()));
            }
        } else {
            return Err(ApiError::InvalidCkbError("Cell info not found".to_string()));
        }
    } else {
        Err(ApiError::CkbDidocCellNotFound)
    }
}

pub fn generate_random_string(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

pub fn extract_timestamp(message: &str) -> Result<u64> {
    for line in message.lines() {
        if line.starts_with("Timestamp: ") {
            let timestamp_str = line.trim_start_matches("Timestamp: ");
            match timestamp_str.trim().parse() {
                Ok(time) => return Ok(time),
                Err(_) => bail!("Timestamp parse error"),
            }
        }
    }
    bail!("Can't find Timestamp in message!")
}

pub fn timestamp_check(timestamp: u64) -> Result<bool> {
    let current_timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_secs(),
        Err(_) => bail!("Can't get unix timestamp from local"),
    };

    Ok(current_timestamp.saturating_sub(timestamp) < 120) // 2 min check
}

pub fn statement_check(message: &str, index: &IndexActionInputRef) -> Result<bool> {
    for line in message.lines() {
        if line.contains(&index.statement()) {
            return Ok(true);
        }
    }
    bail!("Statement check error")
}

pub fn generate_challenge(
    domain: String,
    ckb_addr: String,
    handle: String,
    index: &PreIndexActionInputRef,
) -> Result<String> {
    let ts = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_secs(),
        Err(_) => bail!("Can't get unix timestamp from local"),
    };
    Ok(format!(
        "Web5 Login\nDomain: {}\nAddress: {}\nHandle: {}\nTimestamp: {}\nStatement: {}",
        domain,
        ckb_addr,
        handle,
        ts,
        index.statement(),
    ))
}
