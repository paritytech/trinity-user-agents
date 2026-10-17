// SPDX-License-Identifier: AGPL-3.0-only
use parking_lot::Mutex;
use std::collections::VecDeque;

use super::*;

fn ticket(byte: u8) -> FileTicket {
    FileTicket::from_bytes(&[byte; 32]).unwrap()
}

fn rpc_error(code: i64) -> HopError {
    HopError::Rpc {
        code,
        message: "scripted remote error".into(),
    }
}

struct ScriptedRpc {
    responses: Mutex<VecDeque<(&'static str, Result<Value, HopError>)>>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl ScriptedRpc {
    fn new(responses: Vec<(&'static str, Result<Value, HopError>)>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn methods(&self) -> Vec<String> {
        self.calls
            .lock()
            .iter()
            .map(|(method, _)| method.clone())
            .collect()
    }
}

#[async_trait]
impl HopRpc for ScriptedRpc {
    async fn call(&self, method: &str, params: Value) -> Result<Value, HopError> {
        self.calls.lock().push((method.into(), params));
        let (expected, result) = self.responses.lock().pop_front().expect("unexpected RPC");
        assert_eq!(method, expected);
        result
    }
}

#[test]
fn frozen_native_envelopes_compaction_and_cid_are_preserved() {
    // iOS VersionedUploadedFileTests and Android CompactionBatchStore vectors.
    assert_eq!(
        &*encode_inline(&[0xde, 0xad]).unwrap(),
        &[0, 0, 8, 0xde, 0xad]
    );
    let encoded = encode_chunked_envelope(300, &[[0xab; 32]]).unwrap();
    let mut expected = vec![0, 1, 0x2c, 1, 0, 0, 0, 0, 0, 0, 4, 0x80];
    expected.extend_from_slice(&[0xab; 32]);
    assert_eq!(&*encoded, &expected);
    let DecodedRoot::Chunked(metadata) = decode_root(&encoded).unwrap() else {
        panic!("chunked")
    };
    assert_eq!(metadata.total_size(), 300);
    assert_eq!(metadata.chunks(), &[[0xab; 32]]);
    let mut trailing_root = encoded.to_vec();
    trailing_root.push(0);
    assert!(decode_root(&trailing_root).is_err());
    let mut wrong_hash_length = encoded.to_vec();
    wrong_hash_length[11] = 0x7c; // Declares 31 bytes instead of 32.
    assert!(decode_root(&wrong_hash_length).is_err());
    assert!(decode_root(&[1, 0, 0]).is_err());

    let messages = [vec![0xaa, 0xbb, 0xcc], vec![1, 2]];
    let encoded = encode_compaction_entry(&messages).unwrap();
    assert_eq!(hex::encode(&*encoded), "000020080caabbcc080102");
    let batch = decode_compaction_entry(encoded).unwrap();
    assert_eq!(
        batch.messages().collect::<Vec<_>>(),
        vec![&messages[0][..], &messages[1][..]]
    );

    // Frozen Swift BitswapRemoteStoreTests vector (digest bytes 1...32).
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = index as u8 + 1;
    }
    assert_eq!(
        raw_cid(&digest),
        "bafk2bzaceaaqeayeaudaocajbifqydiob4ibceqtcqkrmfyydenbwha5dypsa"
    );
}

#[test]
fn compaction_preflights_counts_trailing_data_and_exact_inline_boundary() {
    let malformed = [
        vec![1, 0, 0],                         // Unknown envelope version.
        vec![0, 2, 0],                         // Unknown payload index.
        vec![0, 0, 4, 0, 0],                   // Trailing envelope byte.
        vec![0, 0, 8, 0, 0],                   // Trailing inner batch byte.
        vec![0, 0, 4, 4],                      // One message, no length/data.
        vec![0, 0, 20, 3, 255, 255, 255, 255], // Huge count in tiny input.
        vec![0, 0, 5, 0, 0],                   // Noncanonical compact length.
    ];
    for bytes in malformed {
        assert!(decode_compaction_entry(Zeroizing::new(bytes)).is_err());
    }
    let chunked = encode_chunked_envelope(1, &[[0; 32]]).unwrap();
    assert!(decode_compaction_entry(chunked).is_err());

    let exact = vec![0x7b; HOP_INLINE_MAX_BYTES - 5];
    let messages = [exact, vec![9]];
    let batches = pack_compaction_batches(&messages).unwrap();
    assert_eq!(batches, vec![0..1, 1..2]);
    let encoded = encode_compaction_entry(&messages[batches[0].clone()]).unwrap();
    let decoded = decode_compaction_entry(encoded).unwrap();
    assert_eq!(decoded.messages().next().unwrap(), messages[0]);
    assert!(encode_compaction_entry(&messages).is_err());
    assert!(pack_compaction_batches(&[vec![0; HOP_INLINE_MAX_BYTES - 4]]).is_err());

    // Crossing the outer compact-count boundary also consumes a byte.
    let empty_messages: Vec<Vec<u8>> = vec![vec![]; 64];
    let encoded = encode_compaction_entry(&empty_messages).unwrap();
    let decoded = decode_compaction_entry(encoded).unwrap();
    assert_eq!(decoded.message_count(), 64);
    assert!(decoded.messages().all(|message| message.is_empty()));
}

#[test]
fn ticket_authentication_binds_nonce_ciphertext_tag_and_proof_domain() {
    let ticket = ticket(7);
    let clear = b"native attachment bytes";
    let encrypted = ticket.encrypt_with_nonce(clear, [3; 12]).unwrap();
    assert_eq!(&encrypted[..12], &[3; 12]);
    assert_eq!(&*ticket.decrypt(&encrypted).unwrap(), clear);
    assert!(super::tests::ticket(8).decrypt(&encrypted).is_err());
    for index in [0, 12, encrypted.len() - 1] {
        let mut modified = encrypted.clone();
        modified[index] ^= 1;
        assert!(ticket.decrypt(&modified).is_err());
    }
    assert!(ticket.decrypt(&encrypted[..27]).is_err());

    let hash = blake2b_256(&encrypted);
    let (public, claim) = ticket.recipient_proof(&hash, CLAIM_CONTEXT).unwrap();
    let (_, ack) = ticket.recipient_proof(&hash, ACK_CONTEXT).unwrap();
    let MultiSignature::Sr25519(claim) = claim;
    let MultiSignature::Sr25519(ack) = ack;
    let public = schnorrkel::PublicKey::from_bytes(&public).unwrap();
    let mut native_claim_payload = b"hop-claim-v1:".to_vec();
    native_claim_payload.extend_from_slice(&hash);
    let payload = blake2b_256(&native_claim_payload);
    public
        .verify(
            signing_context(b"substrate").bytes(&payload),
            &schnorrkel::Signature::from_bytes(&claim).unwrap(),
        )
        .unwrap();
    assert!(
        public
            .verify(
                signing_context(b"substrate").bytes(&payload),
                &schnorrkel::Signature::from_bytes(&ack).unwrap(),
            )
            .is_err()
    );
}

#[test]
fn pool_claim_leaves_restartable_ack_pending_until_explicit_custody() {
    futures::executor::block_on(async {
        let ticket = ticket(7);
        let prepared = PreparedUpload::inline(b"durable first", &ticket).unwrap();
        let rpc = ScriptedRpc::new(vec![
            ("hop_claim", Ok(json!(prefixed_hex(&prepared.encrypted)))),
            ("hop_ack", Err(rpc_error(HOP_NOT_FOUND))),
        ]);
        let client = HopClient::new(&rpc);
        let claimed = client
            .claim_root(prepared.hash(), &ticket, Some(13))
            .await
            .unwrap();
        assert_eq!(&**claimed.inline.as_ref().unwrap(), b"durable first");
        assert_eq!(rpc.methods(), ["hop_claim"]);
        let persisted = claimed.pending_ack.unwrap().encode();
        let pending = PendingAck::restore(&persisted).unwrap();
        assert_eq!(pending.hash(), prepared.hash());
        // A different ticket must not acknowledge the restored token.
        assert!(
            client
                .acknowledge(&pending, &super::tests::ticket(8))
                .await
                .is_err()
        );
        assert_eq!(rpc.methods(), ["hop_claim"]);
        client.acknowledge(&pending, &ticket).await.unwrap();
        assert_eq!(rpc.methods(), ["hop_claim", "hop_ack"]);
        let mut trailing = persisted;
        trailing.push(0);
        assert!(PendingAck::restore(&trailing).is_err());
    });
}

#[test]
fn only_1004_falls_back_and_promoted_authenticated_entries_never_gain_acks() {
    futures::executor::block_on(async {
        let ticket = ticket(7);
        let prepared = PreparedUpload::compaction(&[b"secret memo"], &ticket).unwrap();
        let rpc = ScriptedRpc::new(vec![
            ("hop_claim", Err(rpc_error(HOP_NOT_FOUND))),
            (
                "bitswap_v1_get",
                Ok(json!(prefixed_hex(&prepared.encrypted))),
            ),
        ]);
        let claimed = HopClient::new(&rpc)
            .claim_compaction(prepared.hash(), &ticket)
            .await
            .unwrap();
        assert!(claimed.pending_ack.is_none());
        assert_eq!(claimed.batch.messages().next().unwrap(), b"secret memo");
        assert_eq!(rpc.methods(), ["hop_claim", "bitswap_v1_get"]);
        assert_eq!(rpc.calls.lock()[1].1, json!([raw_cid(&prepared.hash())]));

        let denied = ScriptedRpc::new(vec![("hop_claim", Err(rpc_error(1003)))]);
        assert!(matches!(
            HopClient::new(&denied)
                .claim_compaction(prepared.hash(), &ticket)
                .await,
            Err(HopError::Rpc { code: 1003, .. })
        ));
        assert_eq!(denied.methods(), ["hop_claim"]);

        let invalid_cid = ScriptedRpc::new(vec![
            ("hop_claim", Err(rpc_error(HOP_NOT_FOUND))),
            ("bitswap_v1_get", Err(rpc_error(BITSWAP_INVALID_CID))),
        ]);
        assert!(matches!(
            HopClient::new(&invalid_cid)
                .claim_compaction(prepared.hash(), &ticket)
                .await,
            Err(HopError::Rpc {
                code: BITSWAP_INVALID_CID,
                ..
            })
        ));
    });
}

#[test]
fn forged_pool_and_bitswap_data_and_wrong_tickets_fail_without_ack() {
    futures::executor::block_on(async {
        let ticket = ticket(7);
        let prepared = PreparedUpload::inline(b"authenticated", &ticket).unwrap();
        let altered = PreparedUpload::inline(b"different bytes", &ticket).unwrap();
        for promoted in [false, true] {
            let mut responses = Vec::new();
            if promoted {
                responses.push(("hop_claim", Err(rpc_error(HOP_NOT_FOUND))));
            }
            responses.push((
                if promoted {
                    "bitswap_v1_get"
                } else {
                    "hop_claim"
                },
                Ok(json!(prefixed_hex(&altered.encrypted))),
            ));
            let rpc = ScriptedRpc::new(responses);
            assert!(matches!(
                HopClient::new(&rpc)
                    .claim_root(prepared.hash(), &ticket, None)
                    .await,
                Err(HopError::Integrity)
            ));
            assert!(!rpc.methods().iter().any(|method| method == "hop_ack"));
        }
        let rpc = ScriptedRpc::new(vec![(
            "hop_claim",
            Ok(json!(prefixed_hex(&prepared.encrypted))),
        )]);
        assert!(matches!(
            HopClient::new(&rpc)
                .claim_root(prepared.hash(), &super::tests::ticket(8), None)
                .await,
            Err(HopError::Crypto)
        ));
    });
}

#[test]
fn chunk_download_resumes_from_durable_offset_and_rejects_wrong_final_size() {
    futures::executor::block_on(async {
        let ticket = ticket(7);
        let first = PreparedUpload::chunk(b"abc", &ticket).unwrap();
        let second = PreparedUpload::chunk(b"de", &ticket).unwrap();
        let envelope = encode_chunked_envelope(5, &[first.hash(), second.hash()]).unwrap();
        let root = PreparedUpload::new(&envelope, &ticket).unwrap();
        let rpc = ScriptedRpc::new(vec![
            ("hop_claim", Ok(json!(prefixed_hex(&root.encrypted)))),
            ("hop_claim", Ok(json!(prefixed_hex(&first.encrypted)))),
            ("hop_claim", Ok(json!(prefixed_hex(&second.encrypted)))),
        ]);
        let client = HopClient::new(&rpc);
        let claimed = client
            .claim_root(root.hash(), &ticket, Some(5))
            .await
            .unwrap();
        assert!(claimed.pending_ack.is_some());
        assert!(claimed.inline.is_none());
        let descriptor = RootDescriptor::restore(&claimed.descriptor.encode()).unwrap();
        let first_chunk = client
            .claim_chunk(&descriptor, DownloadProgress::default(), &ticket)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&*first_chunk.data, b"abc");
        assert!(first_chunk.pending_ack.is_some());
        let progress =
            DownloadProgress::restore(&first_chunk.next_progress.encode(), &descriptor).unwrap();
        let second_chunk = client
            .claim_chunk(&descriptor, progress, &ticket)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&*second_chunk.data, b"de");
        assert!(second_chunk.next_progress.is_complete(&descriptor).unwrap());
        assert!(
            client
                .claim_chunk(&descriptor, second_chunk.next_progress, &ticket)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(rpc.methods(), ["hop_claim", "hop_claim", "hop_claim"]);

        let wrong_total = RootDescriptor::Chunked {
            entry_hash: root.hash(),
            metadata: ChunkedFile {
                total_size: 6,
                chunks: vec![first.hash(), second.hash()],
            },
        };
        let rpc = ScriptedRpc::new(vec![(
            "hop_claim",
            Ok(json!(prefixed_hex(&second.encrypted))),
        )]);
        assert!(matches!(
            HopClient::new(&rpc)
                .claim_chunk(&wrong_total, progress, &ticket)
                .await,
            Err(HopError::InvalidProgress)
        ));
    });
}

#[test]
fn resumable_upload_keeps_ciphertext_hash_and_native_u32_size_bound() {
    let ticket = ticket(7);
    let first = PreparedUpload::chunk(b"persist ciphertext before RPC", &ticket).unwrap();
    let restored = PreparedUpload::restore(&first.encode()).unwrap();
    assert_eq!(restored.hash(), first.hash());
    assert_eq!(restored.encrypted, first.encrypted);
    let mut corrupted = first.encode();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(matches!(
        PreparedUpload::restore(&corrupted),
        Err(HopError::Integrity)
    ));

    let mut progress = UploadProgress::new((HOP_CHUNK_BYTES + 3) as u32).unwrap();
    assert!(
        progress
            .record_chunk(first.hash(), HOP_CHUNK_BYTES - 1)
            .is_err()
    );
    assert!(progress.prepare_root(&ticket).is_err());
    progress
        .record_chunk(first.hash(), HOP_CHUNK_BYTES)
        .unwrap();
    let mut resumed = UploadProgress::restore(&progress.encode()).unwrap();
    assert_eq!(
        resumed.next_chunk().unwrap(),
        ChunkRequest {
            index: 1,
            offset: HOP_CHUNK_BYTES as u64,
            byte_len: 3
        }
    );
    resumed.record_chunk([2; 32], 3).unwrap();
    assert!(resumed.next_chunk().is_none());
    let root = resumed.prepare_root(&ticket).unwrap();
    let plaintext = ticket.decrypt(&root.encrypted).unwrap();
    let DecodedRoot::Chunked(metadata) = decode_root(&plaintext).unwrap() else {
        panic!("chunked root")
    };
    assert_eq!(metadata.chunks(), &[first.hash(), [2; 32]]);
    assert_eq!(metadata.total_size(), (HOP_CHUNK_BYTES + 3) as u64);

    let count = HOP_MAX_FILE_BYTES.div_ceil(HOP_CHUNK_BYTES as u64) as usize;
    let largest = encode_chunked_envelope(HOP_MAX_FILE_BYTES, &vec![[0; 32]; count]).unwrap();
    let DecodedRoot::Chunked(largest) = decode_root(&largest).unwrap() else {
        panic!("maximum native file")
    };
    assert_eq!(largest.total_size(), u64::from(u32::MAX));
    assert!(encode_chunked_envelope(HOP_MAX_FILE_BYTES + 1, &vec![[0; 32]; count]).is_err());
}

struct NativeSender {
    keypair: Keypair,
}

#[async_trait]
impl SenderProofProviding for NativeSender {
    async fn proof(&self, hash: &[u8; 32]) -> Result<SenderProof, HopError> {
        let timestamp = 1_700_000_000_000;
        let payload = sender_proof_payload(hash, timestamp);
        Ok(SenderProof {
            sender: MultiSigner::Sr25519(self.keypair.public.to_bytes()),
            signature: MultiSignature::Sr25519(
                self.keypair
                    .sign(signing_context(b"substrate").bytes(&payload))
                    .to_bytes(),
            ),
            submit_timestamp: timestamp,
        })
    }
}

#[test]
fn native_submit_proof_verifies_over_exact_encrypted_entry_and_timestamp() {
    futures::executor::block_on(async {
        let ticket = ticket(7);
        let prepared = PreparedUpload::inline(b"native upload", &ticket).unwrap();
        let sender = NativeSender {
            keypair: super::tests::ticket(8).signing_keypair().unwrap(),
        };
        let rpc = ScriptedRpc::new(vec![(
            "hop_submit",
            Ok(json!({
                "poolStatus": { "entryCount": 1, "totalBytes": 100, "maxBytes": 1_000 }
            })),
        )]);
        let submitted = HopClient::new(&rpc)
            .submit(&prepared, &sender)
            .await
            .unwrap();
        let calls = rpc.calls.lock();
        let params = &calls[0].1;
        let data =
            hex::decode(params["data"].as_str().unwrap().strip_prefix("0x").unwrap()).unwrap();
        assert_eq!(submitted.hash, blake2b_256(&data));
        assert_eq!(
            &*ticket.decrypt(&data).unwrap(),
            &*encode_inline(b"native upload").unwrap()
        );
        let encoded_signature = hex::decode(
            params["signature"]
                .as_str()
                .unwrap()
                .strip_prefix("0x")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(encoded_signature[0], 1);
        let signature = schnorrkel::Signature::from_bytes(&encoded_signature[1..]).unwrap();
        let mut native_payload = b"hop-submit-v1:".to_vec();
        native_payload.extend_from_slice(&blake2b_256(&data));
        native_payload
            .extend_from_slice(&params["submit_timestamp"].as_u64().unwrap().to_le_bytes());
        sender
            .keypair
            .public
            .verify(
                signing_context(b"substrate").bytes(&blake2b_256(&native_payload)),
                &signature,
            )
            .unwrap();
        assert_eq!(
            params["recipients"],
            json!([prefixed_hex(&ticket.recipient().unwrap().encode())])
        );
    });
}
