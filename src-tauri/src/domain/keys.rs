use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    pub id: String,
    pub name: String,
    pub algorithm: String,
    pub fingerprint_sha256: String,
    pub public_key: String,
    pub storage_type: String,
    pub created_at: String,
    pub updated_at: String,
    pub server_count: u32,
    pub passphrase_saved: bool,
}
