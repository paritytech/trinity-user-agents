//! Behavioral custody authority tests through real product and admin dispatch.

use std::sync::Arc;
use parking_lot::Mutex;
use futures::channel::oneshot;
use parity_scale_codec::{DecodeAll, Encode};
use crate::{CallError, v01, versioned::validator as wire};
use crate::platform::{HostInfo, PlatformInfo, ProductContext, SigningHostConfig, ValidatorPlatform};
use crate::frame::{Payload, ProtocolMessage, MESSAGE_TYPE_REQUEST};
use crate::host_core::{FrameSink, SigningHostRuntime};
use crate::test_support::{StubPlatform, test_spawner};

#[derive(Default)]
struct Frames(Mutex<Vec<Vec<u8>>>);
impl FrameSink for Frames {
    fn emit_frame(&self, frame: Vec<u8>) {
        self.0.lock().push(frame);
    }
}
impl Frames {
    fn response<T: DecodeAll>(&self) -> T {
        let bytes = self.0.lock().pop().expect("response emitted");
        let frame = ProtocolMessage::decode_all(&mut bytes.as_slice()).unwrap();
        T::decode_all(&mut frame.payload.value.as_slice()).unwrap()
    }
}

struct Custody {
    authority: Mutex<Option<v01::ValidatorAuthority>>,
    revocations: Mutex<Vec<String>>,
    entered: Mutex<Option<oneshot::Sender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
    authoring: bool,
    full_duties: bool,
}

#[crate::platform::async_trait]
impl ValidatorPlatform for Custody {
    async fn validator_open(
        &self,
        authority: &v01::ValidatorAuthority,
        request: v01::HostValidatorOpenRequest,
    ) -> Result<v01::HostValidatorOpenResponse, v01::HostValidatorError> {
        let v01::HostValidatorOpenRequest::Acquire { genesis } = request else {
            panic!("test only acquires custody")
        };
        *self.authority.lock() = Some(authority.clone());
        Ok(v01::HostValidatorOpenResponse::Acquired {
            handle: "test-owner".into(), ed25519: [1; 32], bandersnatch: [2; 32],
            p256: [3; 33], genesis, parameters: sp_crypto_hashing::blake2_256(&[1]), generation: "1".into(),
            revision: "0".into(), head: [0; 32], recovery: None, intent_count: 0,
            seal_count: 0, latest_seal_slot: None, history_root: [0; 32], authoring: self.authoring, unix_ms: "1".into(),
            full_duty_count: 0, full_duties: self.full_duties,
            protection: v01::ValidatorProtection::OsKeyringWrappedSoftware,
            stages: vec![0, 1, 2], max_recovery_bytes: v01::VALIDATOR_MAX_RECOVERY_BYTES,
            max_chunk_bytes: v01::VALIDATOR_MAX_CHUNK_BYTES,
            max_storage_bytes: "536870912".into(), max_history_bytes: "536870912".into(),
            max_archive_bytes: "536870912".into(),
        })
    }

    async fn validator_commit(
        &self,
        _: &v01::ValidatorAuthority,
        _: v01::HostValidatorCommitRequest,
    ) -> Result<v01::HostValidatorCommitResponse, v01::HostValidatorError> {
        panic!("test does not reach storage")
    }

    async fn validator_sign(
        &self,
        _: &v01::ValidatorAuthority,
        _: v01::HostValidatorSignRequest,
    ) -> Result<v01::HostValidatorSignResponse, v01::HostValidatorError> {
        let release = self.release.lock().take().expect("one sign");
        self.entered.lock().take().unwrap().send(()).unwrap();
        release.await.unwrap();
        // A completed backend result must still be fenced by runtime revocation.
        Ok(v01::HostValidatorSignResponse {
            signature: [7; 64], revision: "1".into(), head: [8; 32],
        })
    }

    async fn validator_vrf_output(
        &self, _: &v01::ValidatorAuthority, _: v01::HostValidatorVrfOutputRequest,
    ) -> Result<v01::HostValidatorVrfOutputResponse, v01::HostValidatorError> {
        panic!("vote-only ownership must refuse authoring before the platform")
    }

    async fn validator_ticket_prove(
        &self, _: &v01::ValidatorAuthority, _: v01::HostValidatorTicketProveRequest,
    ) -> Result<v01::HostValidatorTicketProveResponse, v01::HostValidatorError> {
        panic!("vote-only ownership must refuse authoring before the platform")
    }

    async fn validator_entropy_source(
        &self, _: &v01::ValidatorAuthority, _: v01::HostValidatorEntropySourceRequest,
    ) -> Result<v01::HostValidatorEntropySourceResponse, v01::HostValidatorError> {
        panic!("vote-only ownership must refuse authoring before the platform")
    }

    async fn validator_seal(
        &self, _: &v01::ValidatorAuthority, _: v01::HostValidatorSealRequest,
    ) -> Result<v01::HostValidatorSealResponse, v01::HostValidatorError> {
        panic!("vote-only ownership must refuse authoring before the platform")
    }

    async fn validator_full_sign(
        &self, _: &v01::ValidatorAuthority, _: v01::HostValidatorFullSignRequest,
    ) -> Result<v01::HostValidatorFullSignResponse, v01::HostValidatorError> {
        assert!(self.full_duties, "role 4 is required before the platform");
        let release = self.release.lock().take().expect("one full sign");
        self.entered.lock().take().unwrap().send(()).unwrap();
        release.await.unwrap();
        Ok(v01::HostValidatorFullSignResponse { signature: [7; 64], revision: "1".into(), head: [8; 32] })
    }

    async fn validator_audit_prove(
        &self, _: &v01::ValidatorAuthority, _: v01::HostValidatorAuditProveRequest,
    ) -> Result<v01::HostValidatorAuditProveResponse, v01::HostValidatorError> {
        assert!(self.full_duties, "role 4 is required before the platform");
        Ok(v01::HostValidatorAuditProveResponse { signature: [7; 96] })
    }

    async fn validator_close(
        &self,
        _: &v01::ValidatorAuthority,
        _: v01::HostValidatorCloseRequest,
    ) -> Result<(), v01::HostValidatorError> {
        panic!("revocation uses the synchronous admission fence")
    }

    fn validator_revoke(&self, execution: String) {
        self.revocations.lock().push(execution);
    }
}

fn scoped_adapters(platform: Arc<StubPlatform>, custody: Arc<Custody>) -> crate::host_core::ConnectionAdapters {
    crate::host_core::ConnectionAdapters {
        platform,
        validator_platform: Some(custody),
        chat_platform: None,
        contacts_platform: None,
        permission_status: None,
        permission_grants: Arc::default(),
        chat: Arc::new(crate::runtime::ActionChannel::chat()),
        renderer: Arc::new(crate::runtime::ActionChannel::renderer()),
        pocket_platform: None,
        game_platform: None,
        scanner_platform: None,
        expanded_card: None,
    }
}

fn request<T: Encode>(method: u8, payload: T) -> Vec<u8> {
    ProtocolMessage {
        request_id: format!("validator:{method}"),
        payload: Payload {
            trait_id: 112, method_id: method, message_type: MESSAGE_TYPE_REQUEST,
            value: payload.encode(),
        },
    }.encode()
}

#[test]
fn shared_admin_revokes_the_live_scoped_adapter_and_fences_in_flight_signatures() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform::default());
        let config = SigningHostConfig::new(
            HostInfo { name: "test".into(), icon: None, version: None, platform: v01::HostPlatform::Unknown },
            PlatformInfo::default(), [0; 32], [0xbb; 32], [0xcc; 32], "paseo".into(),
        ).unwrap();
        let host = SigningHostRuntime::new(platform.clone(), config, test_spawner());
        host.activate_local_session(vec![0xab; 32]).await.unwrap();
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let custody = Arc::new(Custody {
            authority: Mutex::new(None), revocations: Mutex::new(vec![]),
            entered: Mutex::new(Some(entered_tx)), release: Mutex::new(Some(release_rx)),
            authoring: false, full_duties: false,
        });
        let sink = Arc::new(Frames::default());
        let product = "validator.paseo";
        let runtime = host.product_runtime_with(
            ProductContext::new(product.into()).unwrap(), scoped_adapters(platform, custody.clone()), sink.clone(),
        );
        runtime.receive_frame(request(0, wire::HostValidatorOpenRequest::V1(
            v01::HostValidatorOpenRequest::Acquire { genesis: [9; 32] },
        ))).await.unwrap();
        let acquired: Result<wire::HostValidatorOpenResponse, CallError<wire::HostValidatorOpenError>> = sink.response();
        assert!(matches!(acquired, Ok(wire::HostValidatorOpenResponse::V1(
            v01::HostValidatorOpenResponse::Acquired { .. }
        ))));
        let authority = custody.authority.lock().clone().unwrap();
        assert_eq!(authority.product_id, product);
        runtime.receive_frame(request(4, wire::HostValidatorVrfOutputRequest::V1(
            v01::HostValidatorVrfOutputRequest {
                handle: "test-owner".into(), parameters: vec![1],
                input: v01::ValidatorSealInput::FallbackSeal { entropy: [0; 32] },
            },
        ))).await.unwrap();
        let eligibility: Result<wire::HostValidatorVrfOutputResponse, CallError<wire::HostValidatorVrfOutputError>> = sink.response();
        assert_eq!(eligibility, Err(CallError::Domain(wire::HostValidatorVrfOutputError::V1(v01::HostValidatorError::Unsupported))));
        runtime.receive_frame(request(5, wire::HostValidatorTicketProveRequest::V1(
            v01::HostValidatorTicketProveRequest {
                handle: "test-owner".into(), parameters: vec![1], ring: vec![[2; 32]],
                index: 0, entropy: [0; 32], attempt: 0,
            },
        ))).await.unwrap();
        let ticket: Result<wire::HostValidatorTicketProveResponse, CallError<wire::HostValidatorTicketProveError>> = sink.response();
        assert_eq!(ticket, Err(CallError::Domain(wire::HostValidatorTicketProveError::V1(v01::HostValidatorError::Unsupported))));
        runtime.receive_frame(request(6, wire::HostValidatorEntropySourceRequest::V1(
            v01::HostValidatorEntropySourceRequest {
                handle: "test-owner".into(), parameters: vec![1], ticket_id: [0; 32],
            },
        ))).await.unwrap();
        let entropy: Result<wire::HostValidatorEntropySourceResponse, CallError<wire::HostValidatorEntropySourceError>> = sink.response();
        assert_eq!(entropy, Err(CallError::Domain(wire::HostValidatorEntropySourceError::V1(v01::HostValidatorError::Unsupported))));
        runtime.receive_frame(request(7, wire::HostValidatorSealRequest::V1(
            v01::HostValidatorSealRequest {
                handle: "test-owner".into(), parameters: vec![1], revision: "0".into(), head: [0; 32],
                input: v01::ValidatorSealInput::FallbackSeal { entropy: [0; 32] },
                recovery: v01::ValidatorRecoveryRef { hash: [6; 32], length: 1 }, payload: vec![1],
            },
        ))).await.unwrap();
        let seal: Result<wire::HostValidatorSealResponse, CallError<wire::HostValidatorSealError>> = sink.response();
        assert_eq!(seal, Err(CallError::Domain(wire::HostValidatorSealError::V1(v01::HostValidatorError::Unsupported))));
        runtime.receive_frame(request(8, wire::HostValidatorFullSignRequest::V1(
            v01::HostValidatorFullSignRequest {
                handle: "test-owner".into(), parameters: vec![1], revision: "0".into(), head: [0; 32],
                duty: v01::ValidatorFullDuty::Judgement { report: [5; 32], valid: true },
                recovery: v01::ValidatorRecoveryRef { hash: [6; 32], length: 1 },
            },
        ))).await.unwrap();
        let full: Result<wire::HostValidatorFullSignResponse, CallError<wire::HostValidatorFullSignError>> = sink.response();
        assert_eq!(full, Err(CallError::Domain(wire::HostValidatorFullSignError::V1(v01::HostValidatorError::NotGranted))));
        runtime.receive_frame(request(9, wire::HostValidatorAuditProveRequest::V1(
            v01::HostValidatorAuditProveRequest {
                handle: "test-owner".into(), parameters: vec![1],
                input: v01::ValidatorAuditInput::Initial { entropy: [0; 32] },
            },
        ))).await.unwrap();
        let audit: Result<wire::HostValidatorAuditProveResponse, CallError<wire::HostValidatorAuditProveError>> = sink.response();
        assert_eq!(audit, Err(CallError::Domain(wire::HostValidatorAuditProveError::V1(v01::HostValidatorError::NotGranted))));
        let sign = runtime.receive_frame(request(2, wire::HostValidatorSignRequest::V1(
            v01::HostValidatorSignRequest {
                handle: "test-owner".into(), revision: "0".into(), head: [0; 32],
                duty: v01::ValidatorGrandpaDuty { stage: 0, set: 0, round: "1".into(), target: [5; 32], state_root: [7; 32], slot: 1 },
                recovery: v01::ValidatorRecoveryRef { hash: [6; 32], length: 1 },
            },
        )));
        let revoke = async {
            entered_rx.await.unwrap();
            let actual = v01::RemotePermission::Validator {
                genesis: [9; 32], account: authority.account, execution: authority.execution_id.clone(),
            };
            let mut wrong_account = authority.account;
            wrong_account[0] ^= 1;
            for (target_product, permission) in [
                ("other.paseo", actual.clone()),
                (product, v01::RemotePermission::Validator {
                    genesis: [9; 32], account: wrong_account, execution: authority.execution_id.clone(),
                }),
                (product, v01::RemotePermission::Validator {
                    genesis: [10; 32], account: authority.account, execution: authority.execution_id.clone(),
                }),
                (product, v01::RemotePermission::Validator {
                    genesis: [9; 32], account: authority.account, execution: "18446744073709551615".into(),
                }),
            ] {
                host.set_permission_authorization_status(
                    target_product,
                    crate::platform::PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest { permission }),
                    crate::platform::PermissionAuthorizationStatus::Denied,
                ).await.unwrap();
                assert!(custody.revocations.lock().is_empty());
            }
            host.set_permission_authorization_status(
                product,
                crate::platform::PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest { permission: actual }),
                crate::platform::PermissionAuthorizationStatus::Denied,
            ).await.unwrap();
            assert!(custody.revocations.lock().iter().all(|execution| execution == &authority.execution_id));
            assert!(!custody.revocations.lock().is_empty());
            release_tx.send(()).unwrap();
        };
        let (dispatched, ()) = futures::join!(sign, revoke);
        dispatched.unwrap();
        let signed: Result<wire::HostValidatorSignResponse, CallError<wire::HostValidatorSignError>> = sink.response();
        assert_eq!(signed, Err(CallError::Domain(wire::HostValidatorSignError::V1(v01::HostValidatorError::Closed))));
        runtime.dispose();
    });
}

#[test]
fn full_authority_requires_role_parameters_and_valid_duties_and_revokes_in_flight() {
    futures::executor::block_on(async {
        for full_duties in [false, true] {
            let platform = Arc::new(StubPlatform::default());
            let config = SigningHostConfig::new(
                HostInfo { name: "full-test".into(), icon: None, version: None, platform: v01::HostPlatform::Unknown },
                PlatformInfo::default(), [0; 32], [0xbb; 32], [0xcc; 32], "paseo".into(),
            ).unwrap();
            let host = SigningHostRuntime::new(platform.clone(), config, test_spawner());
            host.activate_local_session(vec![0xab; 32]).await.unwrap();
            let (entered_tx, entered_rx) = oneshot::channel();
            let (release_tx, release_rx) = oneshot::channel();
            let custody = Arc::new(Custody {
                authority: Mutex::new(None), revocations: Mutex::new(vec![]),
                entered: Mutex::new(Some(entered_tx)), release: Mutex::new(Some(release_rx)),
                authoring: true, full_duties,
            });
            let sink = Arc::new(Frames::default());
            let product = "validator.paseo";
            let runtime = host.product_runtime_with(
                ProductContext::new(product.into()).unwrap(), scoped_adapters(platform, custody.clone()), sink.clone(),
            );
            runtime.receive_frame(request(0, wire::HostValidatorOpenRequest::V1(
                v01::HostValidatorOpenRequest::Acquire { genesis: [9; 32] },
            ))).await.unwrap();
            let acquired: Result<wire::HostValidatorOpenResponse, CallError<wire::HostValidatorOpenError>> = sink.response();
            assert!(acquired.is_ok());
            let audit = v01::HostValidatorAuditProveRequest {
                handle: "test-owner".into(), parameters: vec![1],
                input: v01::ValidatorAuditInput::Initial { entropy: [0; 32] },
            };
            runtime.receive_frame(request(9, wire::HostValidatorAuditProveRequest::V1(audit.clone()))).await.unwrap();
            let result: Result<wire::HostValidatorAuditProveResponse, CallError<wire::HostValidatorAuditProveError>> = sink.response();
            if !full_duties {
                assert_eq!(result, Err(CallError::Domain(wire::HostValidatorAuditProveError::V1(v01::HostValidatorError::NotGranted))));
                runtime.dispose();
                continue
            }
            assert!(result.is_ok());
            let mut wrong = audit;
            wrong.parameters = vec![2];
            runtime.receive_frame(request(9, wire::HostValidatorAuditProveRequest::V1(wrong))).await.unwrap();
            let result: Result<wire::HostValidatorAuditProveResponse, CallError<wire::HostValidatorAuditProveError>> = sink.response();
            assert_eq!(result, Err(CallError::Domain(wire::HostValidatorAuditProveError::V1(v01::HostValidatorError::NotGranted))));
            let duty = v01::HostValidatorFullSignRequest {
                handle: "test-owner".into(), parameters: vec![1], revision: "0".into(), head: [0; 32],
                duty: v01::ValidatorFullDuty::Guarantee { report: [5; 32] },
                recovery: v01::ValidatorRecoveryRef { hash: [6; 32], length: 1 },
            };
            let mut invalid = duty.clone();
            invalid.duty = v01::ValidatorFullDuty::Announcement { anchor: [1; 32], tranche: 0, reports: vec![] };
            runtime.receive_frame(request(8, wire::HostValidatorFullSignRequest::V1(invalid))).await.unwrap();
            let result: Result<wire::HostValidatorFullSignResponse, CallError<wire::HostValidatorFullSignError>> = sink.response();
            assert_eq!(result, Err(CallError::Domain(wire::HostValidatorFullSignError::V1(v01::HostValidatorError::Invalid))));
            let mut foreign = duty.clone();
            foreign.handle = "another-execution".into();
            runtime.receive_frame(request(8, wire::HostValidatorFullSignRequest::V1(foreign))).await.unwrap();
            let result: Result<wire::HostValidatorFullSignResponse, CallError<wire::HostValidatorFullSignError>> = sink.response();
            assert_eq!(result, Err(CallError::Domain(wire::HostValidatorFullSignError::V1(v01::HostValidatorError::Closed))));
            let signing = runtime.receive_frame(request(8, wire::HostValidatorFullSignRequest::V1(duty)));
            let revoke = async {
                entered_rx.await.unwrap();
                let authority = custody.authority.lock().clone().unwrap();
                host.set_permission_authorization_status(product,
                    crate::platform::PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest {
                        permission: v01::RemotePermission::Validator {
                            genesis: [9; 32], account: authority.account, execution: authority.execution_id,
                        },
                    }),
                    crate::platform::PermissionAuthorizationStatus::Denied,
                ).await.unwrap();
                release_tx.send(()).unwrap();
            };
            let (result, ()) = futures::join!(signing, revoke);
            result.unwrap();
            let result: Result<wire::HostValidatorFullSignResponse, CallError<wire::HostValidatorFullSignError>> = sink.response();
            assert_eq!(result, Err(CallError::Domain(wire::HostValidatorFullSignError::V1(v01::HostValidatorError::Closed))));
            assert!(!custody.revocations.lock().is_empty());
            runtime.dispose();
        }
    });
}
