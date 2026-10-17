import AVFoundation
import BlurHash
import Foundation
import ImageIO
import TrUAPIHost
import UIKit
import UniformTypeIdentifiers

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
        case .actions,
             .notice:
            return nil
        }
    }
}

extension InlineDataURL {
    /// The size of the bytes, and the dimensions and the BlurHash of an image or of the first frame of a video.
    var mediaMeta: ChatMediaMeta {
        let facts = ProductVisualFacts(mimeType: mimeType, data: data)

        return ChatMediaMeta(
            mimeType: mimeType,
            sizeBytes: UInt64(data.count),
            width: facts?.width,
            height: facts?.height,
            placeholder: facts?.blurHash,
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

/// The dimensions and the BlurHash of an image, or of the first frame of a video, for the media facts that the
/// product receives. The product shows the BlurHash until the file is there; for a video it is the poster.
struct ProductVisualFacts: Equatable {
    let width: UInt32
    let height: UInt32
    let blurHash: String?

    init?(mimeType: String, data: Data) {
        if mimeType.hasPrefix("image/"), let image = ProductImageFacts(data) {
            self.init(width: image.width, height: image.height, blurHash: image.blurHash)
        } else if mimeType.hasPrefix("video/"), let video = ProductVideoFacts(data, mimeType: mimeType) {
            self.init(width: video.width, height: video.height, blurHash: video.blurHash)
        } else {
            return nil
        }
    }

    private init(width: UInt32, height: UInt32, blurHash: String?) {
        self.width = width
        self.height = height
        self.blurHash = blurHash
    }
}

/// The dimensions of the first frame of a video and its BlurHash.
struct ProductVideoFacts: Equatable {
    let width: UInt32
    let height: UInt32
    let blurHash: String?

    /// The longest time to wait for the first frame, in seconds.
    static let frameWait: TimeInterval = 5

    init?(_ data: Data, mimeType: String) {
        let fileExtension = UTType(mimeType: mimeType)?.preferredFilenameExtension ?? "mp4"
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension(fileExtension)
        guard (try? data.write(to: url)) != nil else {
            return nil
        }
        defer { try? FileManager.default.removeItem(at: url) }

        guard let frame = Self.firstFrame(of: url) else {
            return nil
        }
        width = UInt32(clamping: frame.width)
        height = UInt32(clamping: frame.height)
        blurHash = Self.small(frame).blurHash(numberOfComponents: (4, 3))
    }

    private static func firstFrame(of url: URL) -> CGImage? {
        let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
        generator.appliesPreferredTrackTransform = true
        let result = FrameResult()
        generator.generateCGImageAsynchronously(for: .zero) { image, _, _ in
            result.set(image)
        }
        return result.wait(seconds: frameWait)
    }

    /// The frame at most 32 pixels on its longest side, as the BlurHash of an image is made.
    private static func small(_ frame: CGImage) -> UIImage {
        let scale = 32 / CGFloat(max(frame.width, frame.height, 1))
        let size = CGSize(width: max(1, CGFloat(frame.width) * scale), height: max(1, CGFloat(frame.height) * scale))
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            UIImage(cgImage: frame).draw(in: CGRect(origin: .zero, size: size))
        }
    }
}

/// The frame that the image generator gives on its own queue.
private final class FrameResult: @unchecked Sendable {
    private let done = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var image: CGImage?

    func set(_ image: CGImage?) {
        lock.withLock { self.image = image }
        done.signal()
    }

    func wait(seconds: TimeInterval) -> CGImage? {
        _ = done.wait(timeout: .now() + seconds)
        return lock.withLock { image }
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
