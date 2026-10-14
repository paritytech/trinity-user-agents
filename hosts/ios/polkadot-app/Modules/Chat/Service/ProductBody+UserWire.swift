import BlurHash
import Foundation
import ImageIO
import TrUAPIHost
import UIKit

extension Chat.LocalMessage.Content.ProductBody {
    /// The body that the user sends to the product: the inline media of a rich text, or an inline file (P6.7).
    /// Nil for a body that the user does not send, or for media that is not inline.
    var userWireBody: ChatMessageBody? {
        switch self {
        case let .richText(richText):
            let media = richText.media.compactMap { InlineDataURL($0)?.mediaItem }
            guard !media.isEmpty || richText.text != nil else { return nil }
            return .richText(text: richText.text, media: media)
        case let .file(file):
            guard let inline = InlineDataURL(file.url) else { return nil }
            return .richText(text: file.text, media: [inline.fileItem(name: file.fileName)])
        case .actions:
            return nil
        }
    }
}

extension InlineDataURL {
    /// The size of the bytes, the dimensions and the BlurHash of an image, and the size of a video.
    var mediaMeta: ChatMediaMeta {
        let image = mimeType.hasPrefix("image/") ? Self.imageFacts(of: data) : nil

        return ChatMediaMeta(
            mimeType: mimeType,
            sizeBytes: UInt64(data.count),
            width: image?.width,
            height: image?.height,
            placeholder: image?.blurHash,
            alt: nil
        )
    }

    /// An inline image or video, or nil for another media type.
    var mediaItem: ChatMediaItem? {
        if mimeType.hasPrefix("image/") {
            .image(resource: .inline(mimeType: mimeType, bytes: data), meta: mediaMeta)
        } else if mimeType.hasPrefix("video/") {
            .video(resource: .inline(mimeType: mimeType, bytes: data), meta: mediaMeta)
        } else {
            nil
        }
    }

    func fileItem(name: String) -> ChatMediaItem {
        .file(resource: .inline(mimeType: mimeType, bytes: data), meta: mediaMeta, fileName: name)
    }

    private struct ImageFacts {
        let width: UInt32
        let height: UInt32
        let blurHash: String?
    }

    private static func imageFacts(of data: Data) -> ImageFacts? {
        guard
            let source = CGImageSourceCreateWithData(data as CFData, nil),
            let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
            let width = properties[kCGImagePropertyPixelWidth] as? Int,
            let height = properties[kCGImagePropertyPixelHeight] as? Int
        else {
            return nil
        }

        let thumbnailOptions: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceThumbnailMaxPixelSize: 32
        ]
        let blurHash = CGImageSourceCreateThumbnailAtIndex(source, 0, thumbnailOptions as CFDictionary)
            .flatMap { UIImage(cgImage: $0).blurHash(numberOfComponents: (4, 3)) }

        return ImageFacts(width: UInt32(clamping: width), height: UInt32(clamping: height), blurHash: blurHash)
    }
}
