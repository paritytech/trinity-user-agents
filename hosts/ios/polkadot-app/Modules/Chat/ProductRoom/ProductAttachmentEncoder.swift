import BlurHash
import Foundation
import ImageIO
import UIKit
import UniformTypeIdentifiers

/// An attachment of the user in a product room, as inline bytes (P6.7, 02-DESIGN §3.5).
struct ProductAttachment: Equatable {
    enum Kind: Equatable {
        case image
        case video
        case file(name: String)
    }

    let kind: Kind
    let mimeType: String
    let data: Data

    /// The `data:` URL that the chat store keeps for inline media (02-DESIGN §20 item 4).
    var dataURL: String {
        "data:\(mimeType);base64,\(data.base64EncodedString())"
    }

    /// The content that the room screen sends as the message of the user.
    var content: Chat.LocalMessage.Content {
        switch kind {
        case .image,
             .video:
            .productBody(.richText(.init(text: nil, media: [dataURL])))
        case let .file(name):
            .productBody(.file(.init(
                url: dataURL,
                fileName: name,
                mimeType: mimeType,
                sizeBytes: UInt64(data.count),
                text: nil
            )))
        }
    }
}

enum ProductAttachmentError: Error, Equatable {
    /// The attachment is larger than the inline limit, and the room does not stage attachments (P10.3).
    case tooLarge(bytes: Int)
    /// The attachment is larger than the room can stage: the core bound, or the Bulletin bound that the core gave.
    case aboveStagingLimit(bytes: Int, limit: Int)
    case unreadable
}

/// Whether the room stages an attachment above the inline limit as a preimage (P10.3, 07-LATER §7).
struct ProductAttachmentStagingPolicy: Equatable {
    /// The room declares `preimage_attachments`, and this host lists the feature.
    let allowed: Bool
    /// The largest attachment that the room stages.
    let maxBytes: Int

    /// The largest attachment that the core stages (`CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES`, 8 MiB). The Bulletin
    /// chain can store less in one transaction (O32). Then the core gives its bound when it refuses the
    /// attachment.
    static let coreMaxBytes = 8 * 1_048_576

    /// The largest attachment that the core seals (`CHAT_SEALED_ATTACHMENT_MAX_BYTES`, 32 MiB).
    static let sealedMaxBytes = 32 * 1_048_576

    /// A room that keeps the inline limit.
    static let inlineOnly = ProductAttachmentStagingPolicy(allowed: false, maxBytes: 0)
}

/// Where an attachment of the user goes.
enum ProductAttachmentRoute: Equatable {
    /// Inline bytes in the message, at most 1 MiB.
    case inline(ProductAttachment)
    /// The host stages the bytes as one preimage that the Bulletin allowance of the product pays for, and the
    /// message holds its key.
    case staged(ProductAttachment)
}

/// Makes inline attachments of at most 1 MiB (`CHAT_INLINE_MEDIA_MAX_BYTES`). An image is encoded again as JPEG, then
/// made smaller, until it fits. A video or a file that is too large is refused.
///
/// In a room that stages attachments, an attachment above the inline limit keeps its bytes and goes to the stager
/// (``route(_:staging:)``).
enum ProductAttachmentEncoder {
    static let inlineLimit = 1_048_576

    private static let jpegQualities: [CGFloat] = [0.85, 0.7, 0.5, 0.35]

    /// Picks the route of an attachment: inline up to 1 MiB, else staged when the room allows it.
    static func route(
        _ picked: PickedAttachment,
        staging: ProductAttachmentStagingPolicy
    ) throws -> ProductAttachmentRoute {
        switch picked {
        case let .image(data):
            guard staging.allowed, data.count > inlineLimit else {
                return try .inline(image(data))
            }
            return try .staged(stagedImage(data, limit: staging.maxBytes))
        case let .video(data, mimeType):
            guard staging.allowed, data.count > inlineLimit else {
                return try .inline(video(data, mimeType: mimeType))
            }
            try checkStaging(data, limit: staging.maxBytes)
            return .staged(ProductAttachment(kind: .video, mimeType: mimeType, data: data))
        case let .file(data, name):
            guard staging.allowed, data.count > inlineLimit else {
                return try .inline(file(data, fileName: name))
            }
            try checkStaging(data, limit: staging.maxBytes)
            return .staged(ProductAttachment(kind: .file(name: name), mimeType: mimeType(fileName: name), data: data))
        }
    }

    static func image(_ data: Data) throws -> ProductAttachment {
        try image(data, limit: inlineLimit)
    }

    /// Encodes the image again as JPEG, and makes it smaller, until it is at most `limit` bytes.
    static func image(_ data: Data, limit: Int) throws -> ProductAttachment {
        guard let image = UIImage(data: data) else {
            throw ProductAttachmentError.unreadable
        }

        var candidate = image
        for _ in 0 ..< 6 {
            for quality in jpegQualities {
                if let jpeg = candidate.jpegData(compressionQuality: quality), jpeg.count <= limit {
                    return ProductAttachment(kind: .image, mimeType: "image/jpeg", data: jpeg)
                }
            }
            candidate = candidate.scaled(by: 0.6)
        }

        throw limit == inlineLimit
            ? ProductAttachmentError.tooLarge(bytes: data.count)
            : ProductAttachmentError.aboveStagingLimit(bytes: data.count, limit: limit)
    }

    /// A staged image keeps its bytes and its format. An image above the staging limit is encoded again to fit.
    private static func stagedImage(_ data: Data, limit: Int) throws -> ProductAttachment {
        guard
            let source = CGImageSourceCreateWithData(data as CFData, nil),
            CGImageSourceGetCount(source) > 0,
            let type = CGImageSourceGetType(source).flatMap({ UTType($0 as String) }),
            let mimeType = type.preferredMIMEType
        else {
            throw ProductAttachmentError.unreadable
        }

        guard data.count <= limit else {
            return try image(data, limit: limit)
        }
        return ProductAttachment(kind: .image, mimeType: mimeType, data: data)
    }

    private static func checkStaging(_ data: Data, limit: Int) throws {
        guard data.count <= limit else {
            throw ProductAttachmentError.aboveStagingLimit(bytes: data.count, limit: limit)
        }
    }

    private static func mimeType(fileName: String) -> String {
        UTType(filenameExtension: (fileName as NSString).pathExtension)?.preferredMIMEType
            ?? "application/octet-stream"
    }

    static func video(_ data: Data, mimeType: String) throws -> ProductAttachment {
        guard data.count <= inlineLimit else {
            throw ProductAttachmentError.tooLarge(bytes: data.count)
        }
        return ProductAttachment(kind: .video, mimeType: mimeType, data: data)
    }

    static func file(_ data: Data, fileName: String) throws -> ProductAttachment {
        guard data.count <= inlineLimit else {
            throw ProductAttachmentError.tooLarge(bytes: data.count)
        }

        return ProductAttachment(kind: .file(name: fileName), mimeType: mimeType(fileName: fileName), data: data)
    }
}

private extension UIImage {
    func scaled(by factor: CGFloat) -> UIImage {
        let target = CGSize(width: max(1, size.width * factor), height: max(1, size.height * factor))
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        return UIGraphicsImageRenderer(size: target, format: format).image { _ in
            draw(in: CGRect(origin: .zero, size: target))
        }
    }
}
