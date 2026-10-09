//! Runs the example workers in `rust/guests` against an in-process host.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use futures::executor::block_on;
use futures::{StreamExt, stream};
use truapi::api::{Account, Preimage};
use truapi::versioned::account::{HostGetUserIdError, HostGetUserIdRequest, HostGetUserIdResponse};
use truapi::versioned::preimage::{
    RemotePreimageLookupSubscribeError, RemotePreimageLookupSubscribeItem,
    RemotePreimageLookupSubscribeRequest, RemotePreimageSubmitError, RemotePreimageSubmitRequest,
    RemotePreimageSubmitResponse,
};
use truapi::{CallContext, CallError, CancellationToken, Subscription, WasmEnv, WasmWorker, v01};

#[derive(Default)]
struct TestHost {
    signed_out: bool,
    preimages: Mutex<Vec<Vec<u8>>>,
    lookup_cancel: Mutex<Option<CancellationToken>>,
    lookup_cancelled_when_user_id_asked: Mutex<Option<bool>>,
}

#[truapi::async_trait]
impl Account for TestHost {
    async fn get_user_id(
        &self,
        _cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        let lookup_cancel = self.lookup_cancel.lock().unwrap();
        *self.lookup_cancelled_when_user_id_asked.lock().unwrap() =
            lookup_cancel.as_ref().map(CancellationToken::is_cancelled);
        if self.signed_out {
            return Err(CallError::Domain(HostGetUserIdError::V1(
                v01::HostGetUserIdError::NotConnected,
            )));
        }
        Ok(HostGetUserIdResponse::V1(v01::HostGetUserIdResponse {
            primary_username: "alice.dot".to_string(),
        }))
    }
}

#[truapi::async_trait]
impl Preimage for TestHost {
    async fn submit(
        &self,
        _cx: &CallContext,
        request: RemotePreimageSubmitRequest,
    ) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
        let RemotePreimageSubmitRequest::V1(value) = request;
        self.preimages.lock().unwrap().push(value);
        Ok(RemotePreimageSubmitResponse::V1(b"key".to_vec()))
    }

    /// Unknown first, then resolved, then open until cancelled.
    async fn lookup_subscribe(
        &self,
        cx: &CallContext,
        _request: RemotePreimageLookupSubscribeRequest,
    ) -> Subscription<
        RemotePreimageLookupSubscribeItem,
        CallError<RemotePreimageLookupSubscribeError>,
    > {
        let value = self.preimages.lock().unwrap().last().cloned();
        *self.lookup_cancel.lock().unwrap() = Some(cx.cancel().clone());
        let items = [None, value].map(|value| {
            Ok(RemotePreimageLookupSubscribeItem::V1(
                v01::RemotePreimageLookupSubscribeItem { value },
            ))
        });
        let cancelled = cx.cancel().cancelled();
        Subscription::new(
            stream::iter(items)
                .chain(stream::pending())
                .take_until(cancelled),
        )
    }
}

fn guest(name: &str) -> Vec<u8> {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    let target = BUILT.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../guests/Cargo.toml");
        let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("guests");
        let status = Command::new(env!("CARGO"))
            .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
            .arg("--manifest-path")
            .arg(manifest)
            .arg("--target-dir")
            .arg(&target_dir)
            .status()
            .expect("cargo runs");
        assert!(status.success(), "guest workers failed to build");
        target_dir.join("wasm32-unknown-unknown/release")
    });
    std::fs::read(target.join(format!("{name}_guest.wasm"))).expect("guest worker was built")
}

fn env(host: impl Into<Arc<TestHost>>) -> WasmEnv<TestHost> {
    let mut env = WasmEnv::new(host.into());
    env.link_account();
    env.link_preimage();
    env
}

#[test]
fn a_worker_reaches_its_product_through_typed_calls() {
    let worker = WasmWorker::new(env(TestHost::default()), &guest("hello")).unwrap();

    assert_eq!(
        block_on(worker.run()).map_err(|error| error.to_string()),
        Ok(())
    );
}

#[test]
fn a_domain_error_reaches_the_worker_as_a_typed_error() {
    let host = TestHost {
        signed_out: true,
        ..TestHost::default()
    };
    let worker = WasmWorker::new(env(host), &guest("hello")).unwrap();

    assert_eq!(
        block_on(worker.run()).map_err(|error| error.to_string()),
        Err("worker failed: Domain(NotConnected)".to_string())
    );
}

#[test]
fn a_worker_streams_a_subscription_until_it_has_what_it_waits_for() {
    let host = Arc::new(TestHost::default());
    let worker = WasmWorker::new(env(host.clone()), &guest("preimage")).unwrap();

    let outcome = block_on(worker.run()).map_err(|error| error.to_string());

    let submitted = host.preimages.lock().unwrap().clone();
    assert_eq!(
        (outcome, submitted),
        (Ok(()), vec![b"hello truapi".to_vec()])
    );
}

#[test]
fn dropping_a_subscription_in_the_worker_cancels_it_on_the_host_at_once() {
    let host = Arc::new(TestHost::default());
    let worker = WasmWorker::new(env(host.clone()), &guest("early_stop")).unwrap();

    let outcome = block_on(worker.run()).map_err(|error| error.to_string());

    let cancelled_by_next_call = *host.lookup_cancelled_when_user_id_asked.lock().unwrap();
    assert_eq!((outcome, cancelled_by_next_call), (Ok(()), Some(true)));
}

#[test]
fn a_worker_importing_a_method_the_host_does_not_link_is_rejected_before_it_runs() {
    let env = WasmEnv::new(Arc::new(TestHost::default()));

    let error = WasmWorker::new(env, &guest("hello"))
        .err()
        .map(|error| error.to_string());

    assert_eq!(
        error.as_deref(),
        Some("unknown import `truapi.account_get_user_id`")
    );
}
