//! Product-account reads for another process (`TRUAPI_ACCOUNT_REQUESTS_DIR`).
//!
//! A serve-mode host gives the product it serves product accounts over the
//! product's own connection only. A test harness driving the product needs
//! the same answer without going through the product: which account will this
//! host sign with for `index`? With a requests directory, the host answers
//! every `<name>.request.json` (`{"productId","index"}`) with
//! `<name>.response.json` (`{"productId","derivationIndex","publicKey",
//! "address"}`, or `{"error"}`) and removes the request.
//!
//! The answer comes from the running host's own session: the product's
//! hard-subtree public key, which a signing host derives locally from its root
//! ([`SigningHostRuntime::product_subtree_public_key`], the key
//! `CoreAdmin::get_product_subtree_public_key` returns), soft-derived by the
//! core's own derivation. Only the product this host serves is answered, so the
//! directory does not become a way to enumerate other products' accounts.
//! Files are written to a temporary name and renamed; nothing opens a socket.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use truapi::SigningHostRuntime;
use truapi::host_logic::product_account::{
    derivation_index_bytes, derive_product_public_key, product_public_key_to_address,
};
use truapi::platform::ProductContext;
use truapi::v01;

/// Environment variable naming the requests directory.
pub const ACCOUNT_REQUESTS_DIR_ENV: &str = "TRUAPI_ACCOUNT_REQUESTS_DIR";

const POLL: Duration = Duration::from_millis(25);

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRequest {
    product_id: String,
    index: u32,
}

/// The directory other processes ask through.
#[derive(Debug, Clone)]
pub struct AccountRequests {
    dir: PathBuf,
}

impl AccountRequests {
    /// Read `TRUAPI_ACCOUNT_REQUESTS_DIR`; `None` when unset.
    pub fn from_env() -> Option<Self> {
        std::env::var_os(ACCOUNT_REQUESTS_DIR_ENV).map(|dir| Self::new(PathBuf::from(dir)))
    }

    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Answer every request present now. `served` is the product this host
    /// serves (normalized); `read` resolves an index of it to a public key.
    pub fn answer_pending<F>(&self, served: &str, read: F) -> usize
    where
        F: Fn(&str, u32) -> Result<[u8; 32], String>,
    {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(".request.json").map(str::to_string))
            .collect();
        names.sort();
        for name in &names {
            let request_path = self.dir.join(format!("{name}.request.json"));
            let answer = match fs::read_to_string(&request_path)
                .map_err(|err| format!("could not read the request: {err}"))
                .and_then(|text| {
                    serde_json::from_str::<AccountRequest>(&text)
                        .map_err(|err| format!("malformed request: {err}"))
                }) {
                Err(error) => serde_json::json!({ "error": error }),
                Ok(request) => answer(served, request, &read),
            };
            let response = self.dir.join(format!("{name}.response.json"));
            if let Err(err) = write_atomically(&response, answer.to_string().as_bytes()) {
                tracing::warn!(path = %response.display(), %err, "could not answer an account request");
            }
            let _ = fs::remove_file(&request_path);
        }
        names.len()
    }

    /// Answer requests until the process ends.
    pub async fn serve(
        self,
        product: Arc<crate::frame_server::ProductSelection>,
        runtime: Arc<SigningHostRuntime>,
    ) {
        if let Err(err) = fs::create_dir_all(&self.dir) {
            tracing::warn!(path = %self.dir.display(), %err, "could not create the account requests directory");
        }
        loop {
            let served = product.current();
            self.answer_pending(&served, |product_id, index| {
                read_product_account(&runtime, product_id, index)
            });
            tokio::time::sleep(POLL).await;
        }
    }
}

/// Write `bytes` to a sibling temporary file, then rename it into place.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

fn answer<F>(served: &str, request: AccountRequest, read: &F) -> serde_json::Value
where
    F: Fn(&str, u32) -> Result<[u8; 32], String>,
{
    let product = match ProductContext::new(request.product_id.clone()) {
        Ok(product) => product.product_id.as_str().to_string(),
        Err(err) => return serde_json::json!({ "error": format!("invalid product id: {err}") }),
    };
    if product != served {
        return serde_json::json!({
            "error": format!("this host serves {served}, not {product}"),
        });
    }
    match read(&product, request.index) {
        Ok(public_key) => serde_json::json!({
            "productId": product,
            "derivationIndex": request.index,
            "publicKey": format!("0x{}", hex::encode(public_key)),
            "address": product_public_key_to_address(public_key),
        }),
        Err(error) => serde_json::json!({ "error": error }),
    }
}

/// The public key the running session gives `product_id` at `index`.
///
/// A signing host derives the subtree from the root it holds, so this neither
/// waits nor needs a timeout.
fn read_product_account(
    runtime: &SigningHostRuntime,
    product_id: &str,
    index: u32,
) -> Result<[u8; 32], String> {
    let subtree = runtime
        .product_subtree_public_key(product_id)
        .map_err(|err| err.reason)?
        .ok_or_else(|| "no active signing session".to_string())?;
    derive_product_public_key(
        subtree,
        derivation_index_bytes(&v01::DerivationIndex::Index(index)),
    )
    .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn response(dir: &Path, name: &str) -> serde_json::Value {
        serde_json::from_str(
            &fs::read_to_string(dir.join(format!("{name}.response.json"))).expect("response"),
        )
        .expect("json")
    }

    #[test]
    fn a_request_for_the_served_product_is_answered_and_removed() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("a.request.json"),
            r#"{"productId":"myapp.paseo","index":1}"#,
        )
        .expect("request");
        let requests = AccountRequests::new(dir.path().to_path_buf());
        let answered = requests.answer_pending("myapp.paseo", |product, index| {
            assert_eq!((product, index), ("myapp.paseo", 1));
            Ok([7u8; 32])
        });
        assert_eq!(answered, 1);
        let answer = response(dir.path(), "a");
        assert_eq!(answer["productId"], "myapp.paseo");
        assert_eq!(answer["derivationIndex"], 1);
        assert_eq!(answer["publicKey"], format!("0x{}", "07".repeat(32)));
        assert_eq!(
            answer["address"],
            product_public_key_to_address([7u8; 32]).as_str()
        );
        assert!(!dir.path().join("a.request.json").exists());
    }

    #[test]
    fn another_product_is_refused_without_reading() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("b.request.json"),
            r#"{"productId":"other.paseo","index":0}"#,
        )
        .expect("request");
        let requests = AccountRequests::new(dir.path().to_path_buf());
        requests.answer_pending("myapp.paseo", |_, _| {
            panic!("another product's account must not be read")
        });
        let answer = response(dir.path(), "b");
        assert!(
            answer["error"]
                .as_str()
                .expect("error")
                .contains("serves myapp.paseo, not other.paseo"),
            "{answer}"
        );
    }

    #[test]
    fn malformed_requests_and_read_failures_are_answered_with_an_error() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("c.request.json"), "{").expect("request");
        fs::write(
            dir.path().join("d.request.json"),
            r#"{"productId":"myapp.paseo","index":0}"#,
        )
        .expect("request");
        let requests = AccountRequests::new(dir.path().to_path_buf());
        requests.answer_pending("myapp.paseo", |_, _| {
            Err("no active signing session".to_string())
        });
        assert!(
            response(dir.path(), "c")["error"]
                .as_str()
                .expect("error")
                .starts_with("malformed request")
        );
        assert_eq!(
            response(dir.path(), "d")["error"],
            "no active signing session"
        );
    }

    #[test]
    fn a_missing_directory_answers_nothing() {
        let dir = tempdir().expect("tempdir");
        let requests = AccountRequests::new(dir.path().join("absent"));
        assert_eq!(
            requests.answer_pending("myapp.paseo", |_, _| Ok([0u8; 32])),
            0
        );
    }

    fn signing_runtime() -> SigningHostRuntime {
        let network = crate::network::Network::default().config();
        let platform = crate::platform::CliPlatform::new(
            network,
            None,
            crate::platform::ApprovalPolicy::AutoAccept,
            None,
        );
        let config = truapi::platform::SigningHostConfig::new(
            truapi::platform::HostInfo {
                name: "Product account requests test".into(),
                icon: None,
                version: None,
                platform: truapi::latest::HostPlatform::Cli,
            },
            truapi::platform::PlatformInfo {
                kind: Some("test".into()),
                version: None,
            },
            network.people_genesis,
            network.bulletin_genesis,
            network.asset_hub_genesis,
            network.network_suffix.to_string(),
        )
        .expect("the signing host config is valid");
        let spawner: truapi::subscription::Spawner = Arc::new(|_| {});
        SigningHostRuntime::new(platform, config, spawner)
    }

    /// The local derivation the reader uses must name the account the core
    /// admin surface names, which is the account a signature carries.
    #[tokio::test]
    async fn the_answer_is_the_core_admin_subtree_soft_derived() {
        use truapi::platform::CoreAdmin;

        let runtime = signing_runtime();
        assert_eq!(
            read_product_account(&runtime, "myapp.paseo", 0),
            Err("no active signing session".to_string())
        );
        runtime
            .activate_local_session_with_identity(vec![7u8; 16], None)
            .await
            .expect("a local session activates");

        let admin = runtime
            .product_admin(ProductContext::new("myapp.paseo".into()).expect("product id"))
            .get_product_subtree_public_key("myapp.paseo".into(), Some(1_000))
            .await
            .expect("the core admin answers")
            .expect("a session is active");
        let local = runtime
            .product_subtree_public_key("myapp.paseo")
            .expect("the local derivation answers");
        assert_eq!(local, Some(admin));
        for index in [0, 3] {
            let expected = derive_product_public_key(
                admin,
                derivation_index_bytes(&v01::DerivationIndex::Index(index)),
            )
            .expect("soft derivation");
            assert_eq!(
                read_product_account(&runtime, "myapp.paseo", index),
                Ok(expected)
            );
        }
        assert_ne!(
            read_product_account(&runtime, "other.paseo", 0),
            read_product_account(&runtime, "myapp.paseo", 0)
        );
    }
}
