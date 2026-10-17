import AVFoundation
import Foundation
import ImageIO
import os
import TrUAPIHost
import UIKit
import UniformTypeIdentifiers

/// The facts of a staged media item: the product receives them with the preimage key (`ChatMediaMeta`).
struct ProductStagedMedia: Equatable, Sendable {
    enum Kind: Equatable, Sendable {
        case image
        case video
        case file
    }

    let kind: Kind
    let mimeType: String
    let sizeBytes: Int
    let width: UInt32?
    let height: UInt32?
    let placeholder: String?

    init(_ entry: ProductOutboxEntry) {
        kind =
            switch entry.kind {
            case .image: .image
            case .video: .video
            case .file: .file
            }
        mimeType = entry.mimeType
        sizeBytes = entry.sizeBytes
        width = entry.width
        height = entry.height
        placeholder = entry.placeholder
    }

    var meta: ChatMediaMeta {
        ChatMediaMeta(
            mimeType: mimeType,
            sizeBytes: UInt64(sizeBytes),
            width: width,
            height: height,
            placeholder: placeholder,
            alt: nil
        )
    }
}

/// The facts of the media that the stager staged, by preimage key, or by the digest of a sealed file. The body of a
/// host-store message keeps only the resource, so the event to the product reads the facts here.
final class ProductStagedMediaRegistry: @unchecked Sendable {
    static let shared = ProductStagedMediaRegistry()

    private let media = OSAllocatedUnfairLock<[Data: ProductStagedMedia]>(initialState: [:])

    func register(key: Data, meta: ProductStagedMedia) {
        media.withLock { $0[key] = meta }
    }

    func media(for key: Data) -> ProductStagedMedia? {
        media.withLock { $0[key] }
    }

    /// The media item of a preimage source (`preimage:<hex>`) or of a sealed source, or nil for another source. A
    /// resource with no facts here goes with the facts that the source gives: a file name, or nothing.
    func mediaItem(source: String, fileName: String?) -> ChatMediaItem? {
        switch ChatResource(source: source) {
        case let .preimage(key)?:
            mediaItem(.preimage(key: key), facts: media(for: key), fileName: fileName)
        case let .sealed(sealed)?:
            mediaItem(sealed.wireResource, facts: media(for: sealed.digest), fileName: fileName)
        default:
            nil
        }
    }

    private func mediaItem(
        _ resource: TrUAPIChatResource,
        facts staged: ProductStagedMedia?,
        fileName: String?
    ) -> ChatMediaItem {
        let meta = staged?.meta ?? ChatMediaMeta(
            mimeType: "application/octet-stream",
            sizeBytes: nil,
            width: nil,
            height: nil,
            placeholder: nil,
            alt: nil
        )
        if let fileName {
            return .file(resource: resource, meta: meta, fileName: fileName)
        }
        return staged?.kind == .video
            ? .video(resource: resource, meta: meta)
            : .image(resource: resource, meta: meta)
    }
}

extension ChatSealedResource {
    /// The resource that the product receives.
    var wireResource: TrUAPIChatResource {
        .sealed(parts: parts, key: key, nonce: nonce, size: size, digest: digest)
    }

    init?(_ resource: TrUAPIChatResource) {
        guard case let .sealed(parts, key, nonce, size, digest) = resource else { return nil }
        self.init(parts: parts, key: key, nonce: nonce, size: size, digest: digest)
    }
}

/// The dimensions and the BlurHash of the bytes of an attachment: an image, or the first frame of a video.
struct ProductStagedMediaFacts {
    let width: UInt32?
    let height: UInt32?
    let blurHash: String?

    init(mimeType: String, data: Data) {
        let facts = ProductVisualFacts(mimeType: mimeType, data: data)
        width = facts?.width
        height = facts?.height
        blurHash = facts?.blurHash
    }
}

/// The bodies of a staged attachment: the bubble while the host stages it, and the message that the product gets.
enum ProductOutboxPreview {
    /// The longest side of the picture in the bubble of an attachment on its way, in pixels.
    static let thumbnailSide = 480

    /// The bubble of an attachment on its way, or of a failed one: a small inline picture, and a line that tells
    /// the state. The product never receives this body.
    static func body(of entry: ProductOutboxEntry, bytes: Data, fileURL: URL?) async -> ChatMessageBody {
        let caption = entry.isFailed
            ? String(localized: .ChatExtension.attachmentStagingFailed)
            : String(localized: .ChatExtension.attachmentStagingSending)

        switch entry.kind {
        case .image:
            let picture = thumbnail(ofImage: bytes)
            return .richText(text: caption, media: picture.map { [pictureItem($0)] } ?? [])
        case .video:
            let picture = if let fileURL { await thumbnail(ofVideo: fileURL) } else { Data?.none }
            return .richText(text: caption, media: picture.map { [pictureItem($0)] } ?? [])
        case let .file(name):
            let meta = ProductStagedMedia(entry).meta
            return .richText(
                text: caption,
                media: [.file(resource: .inline(mimeType: entry.mimeType, bytes: Data()), meta: meta, fileName: name)]
            )
        }
    }

    /// The message that the product gets: one media item with the preimage key or the sealed file, and the facts
    /// of the bytes.
    static func sentBody(of entry: ProductOutboxEntry, resource: TrUAPIChatResource) -> ChatMessageBody {
        let meta = ProductStagedMedia(entry).meta
        let item: ChatMediaItem =
            switch entry.kind {
            case .image: .image(resource: resource, meta: meta)
            case .video: .video(resource: resource, meta: meta)
            case let .file(name): .file(resource: resource, meta: meta, fileName: name)
            }
        return .richText(text: nil, media: [item])
    }

    private static func pictureItem(_ jpeg: Data) -> ChatMediaItem {
        .image(
            resource: .inline(mimeType: "image/jpeg", bytes: jpeg),
            meta: ChatMediaMeta(
                mimeType: "image/jpeg",
                sizeBytes: UInt64(jpeg.count),
                width: nil,
                height: nil,
                placeholder: nil,
                alt: nil
            )
        )
    }

    private static func thumbnail(ofImage data: Data) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: thumbnailSide
        ] as CFDictionary
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options)
            .flatMap { UIImage(cgImage: $0).jpegData(compressionQuality: 0.6) }
    }

    private static func thumbnail(ofVideo url: URL) async -> Data? {
        let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: thumbnailSide, height: thumbnailSide)
        let frame = try? await generator.image(at: .zero).image
        return frame.flatMap { UIImage(cgImage: $0).jpegData(compressionQuality: 0.6) }
    }
}
