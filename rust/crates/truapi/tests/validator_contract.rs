//! Runtime failure behavior when no protected custody backend is installed.
use std::sync::Arc;
use parking_lot::Mutex;
use parity_scale_codec::{Decode, DecodeAll, Encode};
use truapi::{latest, versioned::validator, CallError};
use truapi::{FrameSink, PairingHostRuntime};
use truapi::frame::{Payload, ProtocolMessage, MESSAGE_TYPE_REQUEST};

#[allow(dead_code)]
mod common;

#[derive(Default)]
struct Sink(Mutex<Vec<Vec<u8>>>);
impl FrameSink for Sink {
    fn emit_frame(&self, frame: Vec<u8>) { self.0.lock().push(frame); }
}

#[test]
fn unsupported_host_refuses_custody_through_the_actual_product_dispatcher() {
    let (config, product) = common::test_runtime_config();
    let runtime = PairingHostRuntime::new(
        Arc::new(common::WireShapePlatform), config, common::test_spawner());
    let sink = Arc::new(Sink::default());
    let product = runtime.product_runtime(product, sink.clone());
    let request = ProtocolMessage {
        request_id: "custody-acquire".into(),
        payload: Payload {
            trait_id: 112, method_id: 0, message_type: MESSAGE_TYPE_REQUEST,
            value: validator::HostValidatorOpenRequest::V1(
                latest::HostValidatorOpenRequest::Acquire { genesis: [1; 32] }).encode(),
        },
    };
    futures::executor::block_on(product.receive_frame(request.encode())).unwrap();
    let frames = sink.0.lock();
    let response = frames.iter().map(|frame| ProtocolMessage::decode(&mut &frame[..]).unwrap())
        .find(|frame| frame.request_id == "custody-acquire").expect("custody refusal");
    let result = Result::<validator::HostValidatorOpenResponse,
        CallError<validator::HostValidatorOpenError>>::decode_all(&mut &response.payload.value[..]).unwrap();
    assert_eq!(result, Err(CallError::Domain(validator::HostValidatorOpenError::V1(
        latest::HostValidatorError::Unsupported))));
}

#[test]
fn validator_permission_preserves_the_prototype_discriminant() {
    let permission = latest::RemotePermission::Validator {
        genesis: [1; 32],
        account: [2; 32],
        execution: "7".into(),
    };
    let mut expected = vec![6];
    expected.extend([1; 32]);
    expected.extend([2; 32]);
    expected.extend([4, b'7']);
    assert_eq!(permission.encode(), expected);
    assert_eq!(
        latest::RemotePermission::decode_all(&mut &expected[..]).unwrap(),
        permission,
    );
    assert!(latest::RemotePermission::decode_all(&mut &[5u8][..]).is_err());
}
