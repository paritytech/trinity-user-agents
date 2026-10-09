//! Opt-in passive evidence of signatures returned by a native host.
//!
//! No request payload or secret material is recorded. A transcript failure is
//! reported through tracing and never changes the signing result.

#[cfg(not(target_arch = "wasm32"))]
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(not(target_arch = "wasm32"))]
static NEXT_SIGNATURE_ID: AtomicU64 = AtomicU64::new(1);
#[cfg(not(target_arch = "wasm32"))]
static TRANSCRIPT_LOCK: Mutex<()> = Mutex::new(());

/// Record only after the operation has produced the exact response it returns.
/// `signature` is the returned raw signature or the typed signature embedded in
/// a returned transaction; it is never reconstructed from a payload digest.
pub fn record(
    action: &str,
    payload: &str,
    signature: &[u8],
    signer: &[u8; 32],
    transaction: Option<&[u8]>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(path) = std::env::var_os("TRUAPI_SIGNATURES_LOG") else {
            return;
        };
        let id = format!(
            "{}-{}",
            std::process::id(),
            NEXT_SIGNATURE_ID.fetch_add(1, Ordering::Relaxed)
        );
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let row = row(&id, at, action, payload, signature, signer, transaction);
        if let Err(error) = append(Path::new(&path), &row) {
            tracing::warn!(%error, "could not append TRUAPI_SIGNATURES_LOG evidence");
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (action, payload, signature, signer, transaction);
}

#[cfg(not(target_arch = "wasm32"))]
fn row(
    id: &str,
    at: u128,
    action: &str,
    payload: &str,
    signature: &[u8],
    signer: &[u8; 32],
    transaction: Option<&[u8]>,
) -> serde_json::Value {
    let mut row = serde_json::json!({
        "id": id,
        "at": at,
        "approved": true,
        "action": action,
        "payload": payload,
        "signature": format!("0x{}", hex::encode(signature)),
        "signer": format!("0x{}", hex::encode(signer)),
    });
    if let Some(transaction) = transaction {
        row["transaction"] = format!("0x{}", hex::encode(transaction)).into();
    }
    row
}

#[cfg(not(target_arch = "wasm32"))]
fn append(path: &Path, row: &serde_json::Value) -> std::io::Result<()> {
    let _guard = TRANSCRIPT_LOCK
        .lock()
        .map_err(|_| std::io::Error::other("signature transcript lock poisoned"))?;
    let mut line = serde_json::to_vec(row)?;
    line.push(b'\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&line)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn preserves_exact_signature_and_transaction_bytes() {
        let row = row(
            "4-2",
            123,
            "create transaction",
            "transaction",
            &[1, 0xab],
            &[2; 32],
            Some(&[4, 0xcd]),
        );
        assert_eq!(row["signature"], "0x01ab");
        assert_eq!(row["transaction"], "0x04cd");
        assert_eq!(row["signer"], format!("0x{}", "02".repeat(32)));
        assert_eq!(row["id"], "4-2");
        assert_eq!(row["at"], 123);
        assert_eq!(row["approved"], true);
        assert_eq!(row["payload"], "transaction");
        assert!(row.get("detail").is_none());
    }

    #[test]
    fn raw_message_has_no_invented_transaction() {
        let row = row(
            "4-3",
            124,
            "sign raw data",
            "message",
            &[0xff],
            &[3; 32],
            None,
        );
        assert_eq!(row["signature"], "0xff");
        assert_eq!(row["payload"], "message");
        assert!(row.get("transaction").is_none());
    }

    #[test]
    fn append_retains_each_json_line_and_reports_an_unwritable_destination() {
        let path =
            std::env::temp_dir().join(format!("truapi-signatures-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let row = row(
            "4-4",
            125,
            "sign unprotected data",
            "unknown",
            &[0xee],
            &[4; 32],
            None,
        );
        append(&path, &row).unwrap();
        append(&path, &row).unwrap();
        let lines = std::fs::read_to_string(&path).unwrap();
        assert_eq!(lines.lines().count(), 2);
        for line in lines.lines() {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(line).unwrap(),
                row
            );
        }
        std::fs::remove_file(&path).unwrap();
        assert!(append(&std::env::temp_dir(), &row).is_err());
    }
}
