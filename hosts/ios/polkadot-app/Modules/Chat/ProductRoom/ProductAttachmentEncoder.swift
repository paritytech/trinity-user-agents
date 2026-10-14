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
    /// The attachment is larger than the inline limit, and this host has no preimage path yet (P10).
    case tooLarge(bytes: Int)
    case unreadable
}

/// Makes inline attachments of at most 1 MiB (`CHAT_INLINE_MEDIA_MAX_BYTES`). An image is encoded again as JPEG, then
/// made smaller, until it fits. A video or a file that is too large is refused.
enum ProductAttachmentEncoder {
    static let inlineLimit = 1_048_576

    private static let jpegQualities: [CGFloat] = [0.85, 0.7, 0.5, 0.35]

    static func image(_ data: Data) throws -> ProductAttachment {
        guard let image = UIImage(data: data) else {
            throw ProductAttachmentError.unreadable
        }

        var candidate = image
        for _ in 0 ..< 6 {
            for quality in jpegQualities {
                if let jpeg = candidate.jpegData(compressionQuality: quality), jpeg.count <= inlineLimit {
                    return ProductAttachment(kind: .image, mimeType: "image/jpeg", data: jpeg)
                }
            }
            candidate = candidate.scaled(by: 0.6)
        }

        throw ProductAttachmentError.tooLarge(bytes: data.count)
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

        let mimeType = UTType(filenameExtension: (fileName as NSString).pathExtension)?.preferredMIMEType
            ?? "application/octet-stream"
        return ProductAttachment(kind: .file(name: fileName), mimeType: mimeType, data: data)
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
