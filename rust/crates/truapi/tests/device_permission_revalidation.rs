//! Wiring test for OS revalidation of device permissions.
//!
//! The decision matrix itself is unit-tested in
//! `truapi::host_logic::permissions`. What that cannot reach is the path
//! a real request takes: a product frame through the generated dispatcher into
//! the runtime, which has to hand the permission service the adapter installed
//! on the host runtime. A break anywhere along it — an installer that stores
//! nothing, a runtime that builds the service without the adapter — leaves
//! every unit test passing while no request is ever revalidated.
//!
//! So both cases below drive a real `permissions_request_device_permission`
//! frame against a platform whose prompt grants unconditionally, and separate
//! them only by whether an adapter reporting an OS refusal is installed.

use std::sync::{Arc, Mutex};

use parity_scale_codec::{Decode, Encode};

use truapi::frame::{Payload, ProtocolMessage, request_ids};
use truapi::platform::{DevicePermissionStatus, PermissionStatusHost};
use truapi::platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus};
use truapi::v01;
use truapi::{FrameSink, PairingHostRuntime};

// Shared harness; this binary uses only part of it.
#[allow(dead_code)]
mod common;
use common::{WireShapePlatform, test_runtime_config, test_spawner};

/// Frame sink that keeps every emitted frame in send order.
#[derive(Default)]
struct RecordingSink {
    frames: Mutex<Vec<Vec<u8>>>,
}

impl FrameSink for RecordingSink {
    fn emit_frame(&self, frame: Vec<u8>) {
        self.frames.lock().unwrap().push(frame);
    }
}

/// Status adapter that reports one fixed answer for every capability.
struct FixedStatus(DevicePermissionStatus);

#[truapi::platform::async_trait]
impl PermissionStatusHost for FixedStatus {
    async fn device_permission_status(
        &self,
        _request: v01::HostDevicePermissionRequest,
    ) -> Result<DevicePermissionStatus, v01::GenericError> {
        Ok(self.0)
    }
}

/// Drive one camera request through the dispatcher and return what the product
/// is told. `status` is installed on the host runtime when present.
fn request_camera(status: Option<Arc<dyn PermissionStatusHost>>) -> bool {
    let (host_config, product) = test_runtime_config();
    let runtime = PairingHostRuntime::new(
        Arc::new(WireShapePlatform::default()),
        host_config,
        test_spawner(),
    );
    if let Some(status) = status {
        assert!(
            runtime.set_permission_status_host(status),
            "the adapter must install on a fresh runtime",
        );
    }

    let sink = Arc::new(RecordingSink::default());
    let product_runtime = runtime.product_runtime(product, sink.clone());

    let ids = request_ids("permissions_request_device_permission").expect("known request method");
    let value = truapi::versioned::permissions::HostDevicePermissionRequest::V1(
        v01::HostDevicePermissionRequest::Camera,
    )
    .encode();
    let frame = ProtocolMessage {
        request_id: "p:1".into(),
        payload: Payload {
            trait_id: ids.trait_id,
            method_id: ids.method_id,
            message_type: truapi::frame::MESSAGE_TYPE_REQUEST,
            value,
        },
    };
    futures::executor::block_on(product_runtime.receive_frame(frame.encode()))
        .expect("dispatcher accepted the frame");

    let frames = sink.frames.lock().unwrap().clone();
    let response = frames
        .iter()
        .map(|bytes| ProtocolMessage::decode(&mut &bytes[..]).expect("decode emitted frame"))
        .find(|message| {
            message.payload.trait_id == ids.trait_id && message.payload.method_id == ids.method_id
        })
        .expect("dispatcher emitted a device-permission response");

    assert_eq!(
        response.payload.message_type,
        truapi::frame::MESSAGE_TYPE_RESPONSE
    );
    // Assert the whole thing against each possible answer rather than
    // splicing bytes out by index.
    for granted in [true, false] {
        let expected: Result<
            truapi::versioned::permissions::HostDevicePermissionResponse,
            truapi::CallError<truapi::versioned::permissions::HostDevicePermissionError>,
        > = Ok(
            truapi::versioned::permissions::HostDevicePermissionResponse::V1(
                v01::HostDevicePermissionResponse { granted },
            ),
        );
        if response.payload.value == expected.encode() {
            return granted;
        }
    }
    panic!(
        "unexpected device-permission payload {:?}",
        response.payload.value
    )
}

#[test]
fn a_prompt_that_grants_is_reported_as_granted() {
    // Control. `WireShapePlatform` grants every device prompt, so this fixes
    // what the next case is measured against: the same request, same platform,
    // and the only difference is the installed adapter.
    assert!(request_camera(None));
}

#[test]
fn an_os_refusal_reaches_the_product_as_a_denial() {
    assert!(!request_camera(Some(Arc::new(FixedStatus(
        DevicePermissionStatus::Denied
    )))));
}

#[test]
fn an_os_status_of_not_applicable_leaves_the_prompt_deciding() {
    // A host that serves the capability but has no OS gate for it must not
    // have its grants suppressed.
    assert!(request_camera(Some(Arc::new(FixedStatus(
        DevicePermissionStatus::NotApplicable
    )))));
}

/// A host settings screen reads through `HostAdmin`. It must resolve the same
/// gates a request does, or it reports `Authorized` for a capability the
/// request reports as `granted: false` and sends the user looking for a product
/// toggle that was never what blocked them.
#[test]
fn a_status_read_and_a_request_agree_once_the_os_refuses() {
    let (host_config, product) = test_runtime_config();
    let runtime = PairingHostRuntime::new(
        Arc::new(WireShapePlatform::default()),
        host_config,
        test_spawner(),
    );
    assert!(
        runtime.set_permission_status_host(Arc::new(FixedStatus(DevicePermissionStatus::Denied)))
    );

    let admin = runtime.product_admin(product);
    let read = futures::executor::block_on(admin.permission_authorization_status(
        PermissionAuthorizationRequest::Device(v01::HostDevicePermissionRequest::Camera),
    ))
    .expect("status read");

    assert_eq!(read, PermissionAuthorizationStatus::Denied);
}

/// The same read, for a permission with no OS gate behind it, must not be
/// touched by the status adapter.
#[test]
fn a_status_read_of_a_remote_permission_ignores_the_os_gate() {
    let (host_config, product) = test_runtime_config();
    let runtime = PairingHostRuntime::new(
        Arc::new(WireShapePlatform::default()),
        host_config,
        test_spawner(),
    );
    assert!(
        runtime.set_permission_status_host(Arc::new(FixedStatus(DevicePermissionStatus::Denied)))
    );

    let admin = runtime.product_admin(product);
    let read = futures::executor::block_on(
        admin.permission_authorization_status(PermissionAuthorizationRequest::IdentityDisclosure),
    )
    .expect("status read");

    // Nothing stored and no OS gate, so this is still an open question.
    assert_eq!(read, PermissionAuthorizationStatus::NotDetermined);
}
