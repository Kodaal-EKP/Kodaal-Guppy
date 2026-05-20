use chrono::{SecondsFormat, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn uuid() -> String {
    Uuid::new_v4().to_string()
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}
