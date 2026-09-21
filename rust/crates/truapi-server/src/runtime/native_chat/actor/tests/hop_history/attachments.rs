// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use truapi_platform::{
    NativeChatFileExportRequest, NativeChatFilePickRequest, NativeChatFilesHost,
    NativeChatPickedFile, PermissionAuthorizationRequest,
};

struct Files {
    bytes: Vec<u8>,
    picks: AtomicUsize,
    released: AtomicBool,
    output: Mutex<Vec<u8>>,
    completed: AtomicBool,
}
impl Files {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            picks: AtomicUsize::new(0),
            released: AtomicBool::new(false),
            output: Mutex::new(Vec::new()),
            completed: AtomicBool::new(false),
        }
    }
}
#[async_trait::async_trait]
impl NativeChatFilesHost for Files {
    async fn pick_chat_files(
        &self,
        request: NativeChatFilePickRequest,
    ) -> Result<Vec<NativeChatPickedFile>, GenericError> {
        assert_eq!(request.product_id, PRODUCT);
        self.picks.fetch_add(1, Ordering::SeqCst);
        Ok(vec![NativeChatPickedFile {
            source_id: "immutable-fixture".into(),
            metadata: HostNativeChatAttachmentMetadata {
                mime_type: "image/png".into(),
                size_bytes: self.bytes.len() as u32,
                kind: HostNativeChatAttachmentKind::Image {
                    width: 320,
                    height: 240,
                    thumbnail: Some(b"LEHV6nWB2yk8pyo0adR*.7kCMdnj".to_vec()),
                },
            },
        }])
    }
    async fn read_chat_file(
        &self,
        source: String,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, GenericError> {
        assert_eq!(source, "immutable-fixture");
        assert!(!self.released.load(Ordering::SeqCst));
        assert!(length as usize <= hop::HOP_CHUNK_BYTES);
        Ok(self.bytes[offset as usize..offset as usize + length as usize].to_vec())
    }
    async fn release_chat_file(&self, source: String) -> Result<(), GenericError> {
        assert_eq!(source, "immutable-fixture");
        self.released.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn begin_chat_file_export(
        &self,
        _: NativeChatFileExportRequest,
    ) -> Result<Option<String>, GenericError> {
        self.output.lock().clear();
        self.completed.store(false, Ordering::SeqCst);
        Ok(Some("trusted-output".into()))
    }
    async fn write_chat_file_export(
        &self,
        id: String,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<(), GenericError> {
        assert_eq!(id, "trusted-output");
        let mut output = self.output.lock();
        assert_eq!(offset, output.len() as u64);
        assert!(bytes.len() <= hop::HOP_CHUNK_BYTES);
        output.extend(bytes);
        Ok(())
    }
    async fn finish_chat_file_export(&self, _: String) -> Result<(), GenericError> {
        self.completed.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn cancel_chat_file_export(&self, _: String) -> Result<(), GenericError> {
        self.output.lock().clear();
        Ok(())
    }
}

async fn authorize_upload(platform: &StubPlatform) {
    set_background_grants(platform, PermissionAuthorizationStatus::Authorized).await;
    crate::host_logic::permissions::PermissionsService::new(platform, platform, PRODUCT)
        .set_authorization_status(
            &PermissionAuthorizationRequest::Remote(RemotePermissionRequest {
                permission: RemotePermission::PreimageSubmit,
            }),
            PermissionAuthorizationStatus::Authorized,
        )
        .await
        .unwrap();
}

#[test]
fn attachment_acceptance_loss_reuses_ciphertext_and_download_ack_survives_restart() {
    block_on(async {
        let bytes: Vec<_> = (0..hop::HOP_CHUNK_BYTES + 173)
            .map(|i| (i % 251) as u8)
            .collect();
        let files = Arc::new(Files::new(bytes));
        let pool = Pool::default();
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("attachment fixture has no chain RPC"),
            native_chat_files: Some(files.clone()),
            hop_provider: Some(Arc::new(pool.clone())),
            ..Default::default()
        });
        authorize_upload(&platform).await;
        let fixture = Fixture::on_platform(platform.clone());
        let actor = fixture.actor().await;
        actor.delivering.store(true, Ordering::SeqCst);
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let allowance = derive_sr25519_hard_path(
            &fixture.context.entropy,
            &["allowance", "bulletin", PRODUCT],
        )
        .unwrap();
        assert_ne!(allowance.public.to_bytes(), actor.public.account_id);
        *pool.0.expected_sender.lock() = Some(allowance.public.to_bytes());
        actor
            .send_attachments(
                &fixture.context,
                identity.account,
                "stable-file".into(),
                Some("native image".into()),
            )
            .await
            .unwrap();
        actor.drive_files(&fixture.context).await.unwrap(); // cache + prepared ciphertext
        assert!(pool.0.submissions.lock().is_empty());
        pool.0.reject_next_submit.store(true, Ordering::SeqCst);
        actor.drive_files(&fixture.context).await.unwrap(); // accepted, response lost
        assert_eq!(pool.0.submissions.lock().len(), 1);
        assert!(!files.released.load(Ordering::SeqCst));
        fixture.tasks.stop();
        drop(actor);
        let resumed = Fixture::on_platform(platform.clone());
        let actor = resumed.actor().await;
        actor.delivering.store(true, Ordering::SeqCst);
        actor
            .send_attachments(
                &resumed.context,
                identity.account,
                "stable-file".into(),
                Some("native image".into()),
            )
            .await
            .unwrap();
        assert_eq!(files.picks.load(Ordering::SeqCst), 1);
        assert_eq!(
            actor
                .send_attachments(
                    &resumed.context,
                    identity.account,
                    "stable-file".into(),
                    Some("changed".into())
                )
                .await,
            Err(Error::OperationConflict)
        );
        for _ in 0..8 {
            actor.drive_files(&resumed.context).await.unwrap();
        }
        let submissions = pool.0.submissions.lock().clone();
        assert_eq!(submissions.len(), 4); // retried first chunk, second chunk, root
        assert_eq!(submissions[0], submissions[1]);
        assert!(files.released.load(Ordering::SeqCst));
        let view = actor.public_view(&resumed.context, vec![]).await.unwrap();
        let attachment = &view.rich_messages[0].attachments[0];
        assert_eq!(attachment.state, HostNativeChatAttachmentState::Ready);
        actor
            .open_attachment(&resumed.context, attachment.attachment_id)
            .await
            .unwrap();
        assert!(files.completed.load(Ordering::SeqCst));
        assert_eq!(*files.output.lock(), files.bytes);
        let outgoing = actor
            .store
            .read(|state| {
                state
                    .outbox
                    .iter()
                    .find(|entry| matches!(entry.kind, OutgoingKind::Rich(_)))
                    .cloned()
                    .unwrap()
            })
            .await
            .unwrap();
        let wire::V2StatementTransportData::MultiRequest(multi) =
            open_output(&actor, &identity, &outgoing.statement, false, false)
        else {
            panic!("not a native multi request")
        };
        let body = open_body(&actor, &peer, &multi.encrypted_request, &multi.devices_info);
        let exchange = wire::decode_message_exchange_request_plaintext(&body).unwrap();
        let decoded = wire::decode_message(&exchange.messages[0]).unwrap();
        let wire::V2ChatMessageContent::RichText {
            attachments: Some(references),
            ..
        } = &decoded.content
        else {
            panic!("missing native file reference")
        };
        let wire::V2FileVariant::P2pMixnet(reference) = &references[0];
        for secret in [&reference.claim_ticket, &reference.identifier] {
            assert!(
                !view
                    .encode()
                    .windows(secret.len())
                    .any(|window| window == secret)
            );
        }
        assert!(
            actor
                .send(
                    &resumed.context,
                    identity.account,
                    "raw-capability".into(),
                    exchange.messages.clone()
                )
                .await
                .is_err()
        );

        // An independent native peer forwards the authenticated capability. All
        // incoming cache bytes must survive ACK and reopening without pool data.
        let output = Arc::new(Files::new(Vec::new()));
        let receiver_platform = Arc::new(StubPlatform {
            chain_connect_error: Some("receiver fixture has no chain RPC"),
            native_chat_files: Some(output.clone()),
            hop_provider: Some(Arc::new(pool.clone())),
            ..Default::default()
        });
        set_background_grants(
            &receiver_platform,
            PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let receiver = Fixture::on_platform(receiver_platform.clone());
        let receiving = receiver.actor().await;
        receiving.delivering.store(true, Ordering::SeqCst);
        seed_peer(&receiving, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        assert_eq!(
            receiving
                .receive(
                    &receiver.context,
                    &registry,
                    request(
                        &receiving,
                        &identity,
                        &peer,
                        "native-forward",
                        &exchange.messages
                    )
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(pool.0.claims.load(Ordering::SeqCst), 0);
        receiving.drive_files(&receiver.context).await.unwrap(); // root custody, no ACK yet
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 0);
        receiver.tasks.stop();
        drop(receiving);
        let receiver = Fixture::on_platform(receiver_platform.clone());
        let receiving = receiver.actor().await;
        receiving.delivering.store(true, Ordering::SeqCst);
        for _ in 0..6 {
            receiving.drive_files(&receiver.context).await.unwrap();
        }
        assert!(pool.0.entries.lock().is_empty());
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 3);
        let view = receiving
            .public_view(&receiver.context, vec![])
            .await
            .unwrap();
        let attachment = &view.rich_messages[0].attachments[0];
        assert_eq!(attachment.state, HostNativeChatAttachmentState::Ready);
        receiving
            .open_attachment(&receiver.context, attachment.attachment_id)
            .await
            .unwrap();
        assert!(output.completed.load(Ordering::SeqCst));
        assert_eq!(*output.output.lock(), files.bytes);
        let claims = pool.0.claims.load(Ordering::SeqCst);
        let forwarded = wire::encode_rich_text_message(
            "same-file-another-message",
            receiver.timestamp,
            Some("again"),
            Some(references),
        )
        .unwrap();
        assert_eq!(
            receiving
                .receive(
                    &receiver.context,
                    &registry,
                    request(
                        &receiving,
                        &identity,
                        &peer,
                        "another-forward",
                        &[forwarded]
                    )
                )
                .await,
            Err(Error::NetworkUnavailable)
        );
        receiving.drive_files(&receiver.context).await.unwrap();
        assert_eq!(pool.0.claims.load(Ordering::SeqCst), claims);
        receiver.tasks.stop();
        assert_eq!(
            receiving
                .open_attachment(&receiver.context, attachment.attachment_id)
                .await,
            Err(Error::NotConnected)
        );
    });
}
