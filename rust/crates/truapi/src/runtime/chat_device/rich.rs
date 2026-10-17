// SPDX-License-Identifier: AGPL-3.0-only
//! Native attachment capabilities stay private; only validated metadata is public.

use super::*;
use truapi::latest::{
    HostNativeChatAttachmentKind as Kind, HostNativeChatAttachmentMetadata as Metadata,
    HostNativeChatRichMessageKind as MessageKind,
};

pub(crate) const MAX_ATTACHMENTS: usize = 64;

pub(crate) struct FileReference {
    pub(crate) identifier: [u8; 32],
    pub(crate) ticket: Zeroizing<[u8; 32]>,
    pub(crate) endpoint: String,
    pub(crate) metadata: Metadata,
}

pub(crate) struct RichContent {
    pub(crate) message_id: String,
    pub(crate) timestamp: u64,
    pub(crate) kind: MessageKind,
    pub(crate) text: Option<String>,
    pub(crate) files: Vec<FileReference>,
    pub(crate) digest: [u8; 32],
}

pub(crate) fn validate_metadata(metadata: &Metadata) -> Result<(), ChatDeviceError> {
    if metadata.mime_type.trim().is_empty()
        || metadata.mime_type.len() > 256
        || metadata.mime_type.chars().any(char::is_control)
    {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    let thumbnail = match &metadata.kind {
        Kind::File => None,
        Kind::Image {
            width,
            height,
            thumbnail,
        } => {
            if *width == 0 || *height == 0 {
                return Err(ChatDeviceError::InvalidEncoding);
            }
            thumbnail.as_ref()
        }
        Kind::Video { thumbnail, .. } => thumbnail.as_ref(),
    };
    if let Some(bytes) = thumbnail
        && (bytes.len() > 4096 || core::str::from_utf8(bytes).is_err())
    {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    Ok(())
}

pub(super) fn classify(
    message_id: String,
    timestamp: u64,
    kind: MessageKind,
    text: Option<String>,
    files: Vec<chat::V2FileVariant>,
    original: &[u8],
) -> Result<OpenedDeviceMessage, ChatDeviceError> {
    validate_id(&message_id)?;
    if let Some(text) = &text {
        validate_text(text)?;
    }
    match &kind {
        MessageKind::Message => {}
        MessageKind::Reply { message_id } | MessageKind::Edited { message_id } => {
            validate_id(message_id)?;
        }
    }
    if files.is_empty() || files.len() > MAX_ATTACHMENTS {
        return Err(ChatDeviceError::LimitExceeded);
    }
    let mut references = Vec::with_capacity(files.len());
    for file in files {
        // The canonical type zeroizes its ticket even when another file fails.
        let chat::V2FileVariant::P2pMixnet(mut file) = file;
        let identifier = file
            .identifier
            .as_slice()
            .try_into()
            .map_err(|_| ChatDeviceError::InvalidEncoding)?;
        let ticket = Zeroizing::new(core::mem::take(&mut file.claim_ticket));
        let ticket = Zeroizing::new(
            ticket
                .as_slice()
                .try_into()
                .map_err(|_| ChatDeviceError::InvalidEncoding)?,
        );
        let chat::V2NodeEndpoint::WssUrl(endpoint) = &mut file.node;
        if endpoint.len() > 4096 {
            return Err(ChatDeviceError::LimitExceeded);
        }
        let endpoint = core::mem::take(endpoint);
        let (general, kind) = match &mut file.meta {
            chat::V2FileMeta::General(general) => (general, Kind::File),
            chat::V2FileMeta::Image(image) => (
                &mut image.general,
                Kind::Image {
                    width: image.width,
                    height: image.height,
                    thumbnail: image.thumbnail.take(),
                },
            ),
            chat::V2FileMeta::Video(video) => (
                &mut video.general,
                Kind::Video {
                    duration_seconds: video.duration,
                    thumbnail: video.thumbnail.take(),
                },
            ),
        };
        let metadata = Metadata {
            mime_type: core::mem::take(&mut general.mime_type),
            size_bytes: general.file_size,
            kind,
        };
        validate_metadata(&metadata)?;
        references.push(FileReference {
            identifier,
            ticket,
            endpoint,
            metadata,
        });
    }
    Ok(OpenedDeviceMessage::RichContent(RichContent {
        message_id,
        timestamp,
        kind,
        text,
        files: references,
        digest: sp_crypto_hashing::blake2_256(original),
    }))
}
