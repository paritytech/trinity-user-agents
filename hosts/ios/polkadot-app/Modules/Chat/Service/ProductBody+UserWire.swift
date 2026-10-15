import BlurHash
import Foundation
import ImageIO
import TrUAPIHost
import UIKit

extension Chat.LocalMessage.Content.ProductBody {
    /// The body that the user sends to the product: the inline media of a rich text, or an inline file (P6.7).
    /// A media item that the host staged goes as its preimage key, with the facts that the stager kept (P10.3).
    /// Nil for a body that the user does not send, or for media that is not inline and not staged.
    var userWireBody: ChatMessageBody? {
        userWireBody(staged: .shared)
    }

    func userWireBody(staged: ProductStagedMediaRegistry) -> ChatMessageBody? {
        switch self {
        case let .richText(richText):
            let media = richText.media.compactMap { source in
                InlineDataURL(source)?.mediaItem ?? staged.mediaItem(source: source, fileName: nil)
            }
            guard !media.isEmpty || richText.text != nil else { return nil }
            return .richText(text: richText.text, media: media)
        case let .file(file):
            let item = InlineDataURL(file.url)?.fileItem(name: file.fileName)
                ?? staged.mediaItem(source: file.url, fileName: file.fileName)
            guard let item else { return nil }
            return .richText(text: file.text, media: [item])
        case .actions:
            return nil
        }
    }
}

extension InlineDataURL {
    /// The size of the bytes, the dimensions and the BlurHash of an image, and the size of a video.
    var mediaMeta: ChatMediaMeta {
        let image = mimeType.hasPrefix("image/") ? ProductImageFacts(data) : nil

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
}

/// The dimensions and the BlurHash of an image, for the media facts that the product receives.
struct ProductImageFacts: Equatable {
    let width: UInt32
    let height: UInt32
    let blurHash: String?

    init?(_ data: Data) {
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
        self.width = UInt32(clamping: width)
        self.height = UInt32(clamping: height)
        blurHash = CGImageSourceCreateThumbnailAtIndex(source, 0, thumbnailOptions as CFDictionary)
            .flatMap { UIImage(cgImage: $0).blurHash(numberOfComponents: (4, 3)) }
    }
}
