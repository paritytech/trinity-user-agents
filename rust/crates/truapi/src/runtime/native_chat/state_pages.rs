// SPDX-License-Identifier: AGPL-3.0-only
//! Immutable, product/session-scoped pages of public Chat metadata.

use std::{collections::HashMap, sync::Arc};

use futures::lock::Mutex;
use parity_scale_codec::{Compact, Encode, Output};
use truapi::latest::HostNativeChatStatePage;

use super::{ChatError, DeviceKey, Response};

const MAX_METADATA_BYTES: usize = 512 * 1024;
// The enclosing protocol adds its correlation id, address, response leg and
// version. The registry's transport must also bound the complete frame: this
// layer never sees that correlation id. Normal bounded opened data (256 KiB)
// plus metadata leaves substantially more room than this framing reservation.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024 - 1024;

/// Retained by SessionCache, whose product cap bounds the number of snapshots.
/// Replacing the session cache also makes every previous state id unavailable.
#[derive(Default)]
pub(super) struct StatePages {
    snapshots: Mutex<HashMap<DeviceKey, Arc<Snapshot>>>,
}

struct Snapshot {
    // Direct results are removed before this response enters the cache. In
    // particular it never owns authenticated incoming plaintext/payment keys.
    metadata: Response,
    state_id: [u8; 32],
    pages: Vec<Page>,
}

struct Page {
    // An ordinal in the fixed sequence of metadata vectors, not a byte offset.
    cursor: u32,
    end: u32,
}

impl StatePages {
    pub(super) async fn start(
        &self,
        key: DeviceKey,
        mut response: Response,
    ) -> Result<Response, ChatError> {
        if response.state_page.is_some() {
            return Err(ChatError::InvalidRequest);
        }
        let binding = response.binding.take();
        let opened = std::mem::take(&mut response.opened);
        let open_page = response.open_page.take();
        let mut nonce = [0; 32];
        getrandom::getrandom(&mut nonce).map_err(|_| ChatError::StorageUnavailable)?;
        let state_id = metadata_id(&key, &nonce, &response);
        let mut snapshot = Snapshot {
            metadata: response,
            state_id,
            pages: Vec::new(),
        };
        snapshot.plan()?;
        let mut first = snapshot.page(0)?;
        first.binding = binding;
        first.opened = opened;
        first.open_page = open_page;
        check_response_size(&first)?;
        // Validate every element and the original direct output before replacing
        // the previous snapshot. A failed start cannot strand its continuation.
        self.snapshots.lock().await.insert(key, Arc::new(snapshot));
        Ok(first)
    }

    pub(super) async fn read(
        &self,
        key: &DeviceKey,
        state_id: [u8; 32],
        cursor: u32,
    ) -> Result<Response, ChatError> {
        let snapshot = self
            .snapshots
            .lock()
            .await
            .get(key)
            .cloned()
            .filter(|snapshot| snapshot.state_id == state_id)
            .ok_or(ChatError::OperationNotFound)?;
        // Reads neither consume nor refresh a snapshot. Only exact precomputed
        // boundaries are accepted; guessed interior offsets cannot drop items.
        let index = snapshot
            .pages
            .binary_search_by_key(&cursor, |page| page.cursor)
            .map_err(|_| ChatError::InvalidRequest)?;
        snapshot.page(index)
    }

    pub(super) async fn forget(&self, key: &DeviceKey) {
        self.snapshots.lock().await.remove(key);
    }
}

impl Snapshot {
    fn plan(&mut self) -> Result<(), ChatError> {
        let mut empty = self.empty_page();
        // Account for the larger Some(next_cursor) on every nonfinal page.
        empty.state_page = Some(HostNativeChatStatePage {
            state_id: self.state_id,
            cursor: 0,
            next_cursor: Some(0),
        });
        let mut builder = PageBuilder::new(empty.encoded_size())?;
        // Legacy HOP import could place 16 MiB of individually bounded native
        // frames in one request group. Fragment only that grouping, preserving
        // every frame and its peer/direction/request identity. The state id was
        // already computed over the exact original, unfragmented metadata.
        split_migration_messages(&mut self.metadata, MAX_METADATA_BYTES - builder.base)?;
        let metadata = &self.metadata;
        builder.add(&metadata.peers)?;
        builder.add(&metadata.prepared)?;
        builder.add(&metadata.payments)?;
        builder.add(&metadata.rich_messages)?;
        builder.add(&metadata.migration_invitations)?;
        if let Some(migration) = &metadata.migration {
            builder.add(&migration.peers)?;
            builder.add(&migration.invitations)?;
            builder.add(&migration.messages)?;
            builder.add(&migration.acknowledgments)?;
            builder.add(&migration.payments)?;
            builder.add(&migration.rich_messages)?;
        }
        self.pages = builder.finish();
        Ok(())
    }

    fn empty_page(&self) -> Response {
        Response {
            device: self.metadata.device.clone(),
            peers: Vec::new(),
            binding: None,
            opened: Vec::new(),
            prepared: Vec::new(),
            payments: Vec::new(),
            rich_messages: Vec::new(),
            migration: self.metadata.migration.as_ref().map(|migration| {
                truapi::v02::HostProductDeviceChatResponse {
                    device: migration.device.clone(),
                    peers: Vec::new(),
                    invitations: Vec::new(),
                    messages: Vec::new(),
                    acknowledgments: Vec::new(),
                    payments: Vec::new(),
                    rich_messages: Vec::new(),
                }
            }),
            migration_id: self.metadata.migration_id,
            open_page: None,
            migration_invitations: Vec::new(),
            state_page: None,
            coinage_cents_unit: self.metadata.coinage_cents_unit,
        }
    }

    fn page(&self, index: usize) -> Result<Response, ChatError> {
        let page = self.pages.get(index).ok_or(ChatError::InvalidRequest)?;
        let mut result = self.empty_page();
        let mut offset = 0;
        let metadata = &self.metadata;
        result.peers = page_slice(&metadata.peers, &mut offset, page);
        result.prepared = page_slice(&metadata.prepared, &mut offset, page);
        result.payments = page_slice(&metadata.payments, &mut offset, page);
        result.rich_messages = page_slice(&metadata.rich_messages, &mut offset, page);
        result.migration_invitations =
            page_slice(&metadata.migration_invitations, &mut offset, page);
        if let (Some(source), Some(target)) = (&metadata.migration, &mut result.migration) {
            target.peers = page_slice(&source.peers, &mut offset, page);
            target.invitations = page_slice(&source.invitations, &mut offset, page);
            target.messages = page_slice(&source.messages, &mut offset, page);
            target.acknowledgments = page_slice(&source.acknowledgments, &mut offset, page);
            target.payments = page_slice(&source.payments, &mut offset, page);
            target.rich_messages = page_slice(&source.rich_messages, &mut offset, page);
        }
        result.state_page = Some(HostNativeChatStatePage {
            state_id: self.state_id,
            cursor: page.cursor,
            next_cursor: self.pages.get(index + 1).map(|next| next.cursor),
        });
        if result.encoded_size() > MAX_METADATA_BYTES {
            return Err(ChatError::InvalidRequest);
        }
        Ok(result)
    }
}

fn split_migration_messages(metadata: &mut Response, limit: usize) -> Result<(), ChatError> {
    let Some(migration) = &mut metadata.migration else {
        return Ok(());
    };
    let groups = std::mem::take(&mut migration.messages);
    for mut group in groups {
        if group.encoded_size() <= limit {
            migration.messages.push(group);
            continue;
        }
        let messages = std::mem::take(&mut group.messages);
        let base = group.encoded_size();
        if base > limit {
            return Err(ChatError::InvalidRequest);
        }
        let mut bytes = base;
        for message in messages {
            let count =
                u32::try_from(group.messages.len()).map_err(|_| ChatError::InvalidRequest)?;
            let next = count.checked_add(1).ok_or(ChatError::InvalidRequest)?;
            let size = message.encoded_size();
            let growth = Compact(next).encoded_size() - Compact(count).encoded_size();
            let mut added = size.checked_add(growth).ok_or(ChatError::InvalidRequest)?;
            if added > limit - bytes {
                if group.messages.is_empty() || size > limit - base {
                    return Err(ChatError::InvalidRequest);
                }
                let next_group = truapi::latest::HostNativeChatMessages {
                    peer_identity: group.peer_identity,
                    incoming: group.incoming,
                    request_id: group.request_id.clone(),
                    messages: Vec::new(),
                };
                migration
                    .messages
                    .push(std::mem::replace(&mut group, next_group));
                bytes = base;
                added = size;
            }
            bytes += added;
            group.messages.push(message);
        }
        migration.messages.push(group);
    }
    Ok(())
}

/// Add only the overlapping whole public items. Nested fields retain their
/// original associations (e.g. a peer's roster and a request's safe messages).
fn page_slice<T: Clone>(source: &[T], offset: &mut usize, page: &Page) -> Vec<T> {
    let start = (page.cursor as usize)
        .saturating_sub(*offset)
        .min(source.len());
    let end = (page.end as usize)
        .saturating_sub(*offset)
        .min(source.len());
    *offset += source.len();
    source[start..end].to_vec()
}

struct PageBuilder {
    base: usize,
    bytes: usize,
    cursor: u32,
    start: u32,
    pages: Vec<Page>,
}

impl PageBuilder {
    fn new(base: usize) -> Result<Self, ChatError> {
        if base > MAX_METADATA_BYTES {
            return Err(ChatError::InvalidRequest);
        }
        Ok(Self {
            base,
            bytes: base,
            cursor: 0,
            start: 0,
            pages: Vec::new(),
        })
    }

    fn add<T: Encode>(&mut self, items: &[T]) -> Result<(), ChatError> {
        let mut count = 0u32;
        for item in items {
            let size = item.encoded_size();
            let next_count = count.checked_add(1).ok_or(ChatError::InvalidRequest)?;
            let growth = Compact(next_count).encoded_size() - Compact(count).encoded_size();
            let mut added = size.checked_add(growth).ok_or(ChatError::InvalidRequest)?;
            if added > MAX_METADATA_BYTES - self.bytes {
                if self.start == self.cursor {
                    return Err(ChatError::InvalidRequest);
                }
                self.pages.push(Page {
                    cursor: self.start,
                    end: self.cursor,
                });
                self.start = self.cursor;
                self.bytes = self.base;
                count = 0;
                added = size; // Compact(0) and Compact(1) are both one byte.
                if added > MAX_METADATA_BYTES - self.bytes {
                    return Err(ChatError::InvalidRequest);
                }
            }
            self.bytes += added;
            self.cursor = self
                .cursor
                .checked_add(1)
                .ok_or(ChatError::InvalidRequest)?;
            count += 1;
        }
        Ok(())
    }

    fn finish(mut self) -> Vec<Page> {
        // Even empty metadata has one terminal page, with its device and ids.
        self.pages.push(Page {
            cursor: self.start,
            end: self.cursor,
        });
        self.pages
    }
}

fn check_response_size(response: &Response) -> Result<(), ChatError> {
    if response.encoded_size() >= MAX_RESPONSE_BYTES {
        return Err(ChatError::InvalidRequest);
    }
    Ok(())
}

struct DigestOutput(blake2b_simd::State);

impl Output for DigestOutput {
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}

fn metadata_id(key: &DeviceKey, nonce: &[u8; 32], metadata: &Response) -> [u8; 32] {
    let mut output = DigestOutput(blake2b_simd::Params::new().hash_length(32).to_state());
    output.write(b"truapi/native-chat/metadata-pages/v1\0");
    key.encode_to(&mut output);
    nonce.encode_to(&mut output);
    // SCALE binds all metadata fields, including empty vectors, migration
    // presence, both device records, and migration_id, without a huge temporary.
    metadata.encode_to(&mut output);
    let hash = output.0.finalize();
    let mut id = [0; 32];
    id.copy_from_slice(hash.as_bytes());
    id
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use truapi::latest::*;

    use super::*;

    fn key(product: &str) -> DeviceKey {
        (([1; 32], [2; 32]), product.into())
    }

    fn device() -> HostNativeChatDevice {
        HostNativeChatDevice {
            identity_account_id: [3; 32],
            identity_chat_public_key: [4; 32],
            product_account: ProductAccountId {
                dot_ns_identifier: "chat.test".into(),
                derivation_index: DerivationIndex::Raw([0; 32]),
            },
            account_id: [5; 32],
            chat_public_key: [6; 32],
        }
    }

    fn response() -> Response {
        Response {
            device: device(),
            peers: Vec::new(),
            binding: None,
            opened: Vec::new(),
            prepared: Vec::new(),
            payments: Vec::new(),
            rich_messages: Vec::new(),
            migration: None,
            migration_id: None,
            open_page: None,
            migration_invitations: Vec::new(),
            state_page: None,
            coinage_cents_unit: None,
        }
    }

    fn rich(index: usize) -> HostNativeChatRichMessage {
        HostNativeChatRichMessage {
            peer_identity: [7; 32],
            incoming: true,
            request_id: format!("request-{index}"),
            message_id: format!("message-{index}"),
            timestamp: index as u64,
            kind: HostNativeChatRichMessageKind::Message,
            text: Some("x".repeat(8192)),
            attachments: Vec::new(),
        }
    }

    #[test]
    fn replay_boundaries_and_replacement_are_product_and_session_scoped() {
        block_on(async {
            let pages = StatePages::default();
            let mut source = response();
            source.coinage_cents_unit = Some(16_384);
            source.rich_messages = (0..140).map(rich).collect();
            let first = pages.start(key("a"), source).await.unwrap();
            let state = first.state_page.clone().unwrap();
            let next = state.next_cursor.unwrap();
            assert!(next > 1);
            assert_eq!(
                pages.read(&key("a"), state.state_id, 0).await.unwrap(),
                first
            );
            assert_eq!(
                pages.read(&key("a"), state.state_id, 1).await,
                Err(ChatError::InvalidRequest)
            );
            assert_eq!(
                pages.read(&key("a"), state.state_id, u32::MAX).await,
                Err(ChatError::InvalidRequest)
            );
            assert_eq!(
                pages.read(&key("b"), state.state_id, next).await,
                Err(ChatError::OperationNotFound)
            );
            let continuation = pages.read(&key("a"), state.state_id, next).await.unwrap();
            assert_eq!(continuation.coinage_cents_unit, Some(16_384));
            assert_eq!(
                pages.read(&key("a"), state.state_id, next).await.unwrap(),
                continuation
            );
            let mut unrelated = response();
            unrelated.rich_messages = vec![rich(200)];
            pages.start(key("b"), unrelated).await.unwrap();
            assert_eq!(
                pages.read(&key("a"), state.state_id, next).await.unwrap(),
                continuation
            );
            let replacement = pages.start(key("a"), response()).await.unwrap();
            let replacement_id = replacement.state_page.unwrap().state_id;
            assert_eq!(
                pages.read(&key("a"), state.state_id, next).await,
                Err(ChatError::OperationNotFound)
            );
            assert_eq!(
                StatePages::default()
                    .read(&key("a"), replacement_id, 0)
                    .await,
                Err(ChatError::OperationNotFound)
            );
            let identical = pages.start(key("a"), response()).await.unwrap();
            let identical_id = identical.state_page.unwrap().state_id;
            assert_ne!(replacement_id, identical_id);
            assert_eq!(
                pages.read(&key("a"), replacement_id, 0).await,
                Err(ChatError::OperationNotFound)
            );
            pages.forget(&key("a")).await;
            assert_eq!(
                pages.read(&key("a"), identical_id, 0).await,
                Err(ChatError::OperationNotFound)
            );
        });
    }

    #[test]
    fn oversized_metadata_or_direct_output_does_not_replace_a_live_snapshot() {
        block_on(async {
            let pages = StatePages::default();
            let first = pages.start(key("a"), response()).await.unwrap();
            let id = first.state_page.as_ref().unwrap().state_id;
            let mut oversized = response();
            let mut item = rich(0);
            item.text = Some("x".repeat(MAX_METADATA_BYTES));
            oversized.rich_messages = vec![item];
            assert_eq!(
                pages.start(key("a"), oversized).await,
                Err(ChatError::InvalidRequest)
            );
            let mut oversized_frame = response();
            oversized_frame.migration = Some(truapi::v02::HostProductDeviceChatResponse {
                device: device(),
                peers: Vec::new(),
                invitations: Vec::new(),
                messages: vec![HostNativeChatMessages {
                    peer_identity: [7; 32],
                    incoming: true,
                    request_id: "oversized-frame".into(),
                    messages: vec![vec![0; MAX_METADATA_BYTES]],
                }],
                acknowledgments: Vec::new(),
                payments: Vec::new(),
                rich_messages: Vec::new(),
            });
            assert_eq!(
                pages.start(key("a"), oversized_frame).await,
                Err(ChatError::InvalidRequest)
            );
            let mut direct = response();
            direct.opened.push(HostNativeChatOpened {
                peer_identity: [7; 32],
                sender_account_id: [8; 32],
                route: HostNativeChatRoute::Device,
                plaintext: vec![0; MAX_RESPONSE_BYTES],
            });
            assert_eq!(
                pages.start(key("a"), direct).await,
                Err(ChatError::InvalidRequest)
            );
            assert_eq!(pages.read(&key("a"), id, 0).await.unwrap(), first);
        });
    }

    #[test]
    fn migration_and_current_vectors_are_losslessly_paged_without_direct_plaintext() {
        block_on(async {
            let pages = StatePages::default();
            let mut source = response();
            source.peers.push(HostNativeChatPeer {
                identity_account_id: [7; 32],
                username: Some("alice".into()),
                devices: vec![HostNativeChatPeerDevice {
                    account_id: [8; 32],
                    chat_public_key: [9; 32],
                }],
                incoming_channels: vec![[10; 32]],
                ready_for_payments: true,
            });
            source.prepared.push(HostNativeChatPrepared {
                statement: SignedStatement {
                    proof: StatementProof::Sr25519 {
                        signature: [11; 64],
                        signer: [5; 32],
                    },
                    decryption_key: None,
                    expiry: Some(123),
                    channel: Some([12; 32]),
                    topics: vec![[13; 32]],
                    data: Some(vec![14; 100]),
                },
                peer_identity: [7; 32],
                request_id: "prepared".into(),
                requires_ack: true,
                client_request_id: Some("client-prepared".into()),
            });
            source.payments.push(HostNativeChatPayment {
                operation_id: [15; 32],
                request_id: "payment".into(),
                message_id: "card".into(),
                timestamp: 123,
                peer_identity: [7; 32],
                direction: HostNativeChatPaymentDirection::Outgoing,
                amount_cents: 42,
                state: HostNativeChatPaymentState::Delivering,
            });
            source.rich_messages = (0..140).map(rich).collect();
            source
                .migration_invitations
                .push(HostNativeChatMigrationInvitation {
                    invitation_id: [16; 32],
                    request_id: "invite".into(),
                });
            source.migration_id = Some([17; 32]);
            source.migration = Some(truapi::v02::HostProductDeviceChatResponse {
                device: device(),
                peers: source.peers.clone(),
                invitations: vec![HostNativeChatInvitation {
                    invitation_id: [16; 32],
                    peer_identity: [7; 32],
                    username: None,
                    timestamp: 123,
                    text: "hello".into(),
                }],
                messages: (0..140)
                    .map(|index| HostNativeChatMessages {
                        peer_identity: [7; 32],
                        incoming: true,
                        request_id: format!("legacy-{index}"),
                        messages: vec![vec![index as u8; 8192], vec![42; 32]],
                    })
                    .collect(),
                acknowledgments: vec![HostNativeChatAcknowledgment {
                    peer_identity: [7; 32],
                    request_id: "ack".into(),
                    response_code: 0,
                }],
                payments: source.payments.clone(),
                rich_messages: (200..340).map(rich).collect(),
            });
            source
                .migration
                .as_mut()
                .unwrap()
                .messages
                .push(HostNativeChatMessages {
                    peer_identity: [7; 32],
                    incoming: true,
                    request_id: "expanded-hop".into(),
                    messages: (0..140).map(|index| vec![index as u8; 8192]).collect(),
                });
            let expected = source.clone();
            source.binding = Some(HostNativeChatBinding {
                peer_identity: [7; 32],
                peer_chat_public_key: [9; 32],
                identity_proof: [18; 32],
            });
            source.opened.push(HostNativeChatOpened {
                peer_identity: [7; 32],
                sender_account_id: [8; 32],
                route: HostNativeChatRoute::Device,
                plaintext: vec![19; 256 * 1024],
            });
            source.open_page = Some(HostNativeChatOpenPage {
                open_id: [20; 32],
                cursor: 0,
                next_cursor: Some(1),
            });
            let mut page = pages.start(key("a"), source).await.unwrap();
            assert!(page.binding.is_some());
            assert_eq!(page.opened[0].plaintext, vec![19; 256 * 1024]);
            assert!(page.open_page.is_some());
            assert!(page.encoded_size() < MAX_RESPONSE_BYTES);
            let id = page.state_page.as_ref().unwrap().state_id;
            let mut replay = pages.read(&key("a"), id, 0).await.unwrap();
            assert!(
                replay.binding.is_none() && replay.opened.is_empty() && replay.open_page.is_none()
            );
            page.binding = None;
            page.opened.clear();
            page.open_page = None;
            assert_eq!(page, replay);
            let mut combined = response();
            combined.migration_id = expected.migration_id;
            combined.migration = Some(truapi::v02::HostProductDeviceChatResponse {
                device: device(),
                peers: Vec::new(),
                invitations: Vec::new(),
                messages: Vec::new(),
                acknowledgments: Vec::new(),
                payments: Vec::new(),
                rich_messages: Vec::new(),
            });
            loop {
                assert!(replay.encoded_size() <= MAX_METADATA_BYTES);
                assert_eq!(replay.device, expected.device);
                assert_eq!(replay.migration_id, expected.migration_id);
                assert!(
                    replay.opened.is_empty()
                        && replay.binding.is_none()
                        && replay.open_page.is_none()
                );
                let next = replay.state_page.as_ref().unwrap().next_cursor;
                combined.peers.extend(replay.peers);
                combined.prepared.extend(replay.prepared);
                combined.payments.extend(replay.payments);
                combined.rich_messages.extend(replay.rich_messages);
                combined
                    .migration_invitations
                    .extend(replay.migration_invitations);
                let migration = replay.migration.unwrap();
                let target = combined.migration.as_mut().unwrap();
                assert_eq!(migration.device, target.device);
                target.peers.extend(migration.peers);
                target.invitations.extend(migration.invitations);
                for group in migration.messages {
                    if let Some(previous) = target.messages.last_mut().filter(|previous| {
                        previous.peer_identity == group.peer_identity
                            && previous.incoming == group.incoming
                            && previous.request_id == group.request_id
                    }) {
                        previous.messages.extend(group.messages);
                    } else {
                        target.messages.push(group);
                    }
                }
                target.acknowledgments.extend(migration.acknowledgments);
                target.payments.extend(migration.payments);
                target.rich_messages.extend(migration.rich_messages);
                let Some(cursor) = next else { break };
                replay = pages.read(&key("a"), id, cursor).await.unwrap();
            }
            assert_eq!(combined, expected);
        });
    }
}
