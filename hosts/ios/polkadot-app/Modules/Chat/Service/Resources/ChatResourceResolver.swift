import BulletinChain
import CryptoKit
import Foundation
import ImageIO
import UIKit

protocol ChatResourceResolving: Sendable {
    /// The bytes of a resource, or nil when the resource cannot be read now.
    func data(for resource: ChatResource) async -> Data?
    /// The picture of a resource, or nil when the resource is not a picture or cannot be read now.
    func image(for resource: ChatResource) async -> UIImage?
}

protocol ChatResourceEvicting: Sendable {
    /// Removes the resources from the caches, for example when the room that referenced them goes away.
    func evict(_ resources: [ChatResource]) async
}

/// Resolves the resources of chat bodies, room avatars and product widgets through one bounded disk cache.
///
/// Inline bytes need no fetch. A preimage key goes through the preimage lookup. An `https` address goes
/// through `fetch`, and a file address is read in place. A failed read is not cached, so a later read
/// tries again.
final class ChatResourceResolver: ChatResourceResolving, @unchecked Sendable {
    typealias Fetch = @Sendable (URL) async throws -> Data

    private let preimages: TrUAPIPreimageLookuping
    private let cache: ChatResourceCaching
    private let fetch: Fetch
    private let images = NSCache<NSString, UIImage>()

    init(
        preimages: TrUAPIPreimageLookuping,
        cache: ChatResourceCaching,
        fetch: @escaping Fetch = ChatResourceResolver.fetchOverNetwork,
        imageCountLimit: Int = 200
    ) {
        self.preimages = preimages
        self.cache = cache
        self.fetch = fetch
        images.countLimit = imageCountLimit
        images.totalCostLimit = Self.decodedBytesLimit
    }

    func data(for resource: ChatResource) async -> Data? {
        switch resource {
        case let .inline(_, data):
            data
        case let .address(url) where url.isFileURL:
            try? Data(contentsOf: url)
        case .preimage,
             .address:
            await cachedData(for: resource)
        }
    }

    func image(for resource: ChatResource) async -> UIImage? {
        let key = resource.cacheKey as NSString
        if let image = images.object(forKey: key) {
            return image
        }

        guard let data = await data(for: resource), let image = Self.decodedImage(from: data) else {
            return nil
        }

        images.setObject(image, forKey: key, cost: Self.decodedBytes(of: image))
        return image
    }
}

extension ChatResourceResolver: ChatResourceEvicting {
    func evict(_ resources: [ChatResource]) async {
        for resource in resources {
            images.removeObject(forKey: resource.cacheKey as NSString)
            await cache.remove(for: resource.cacheKey)
        }
    }
}

extension ChatResourceResolver {
    /// One resolver for the app, so that chat bodies, room avatars and widgets share one cache.
    static let shared = ChatResourceResolver(
        preimages: TrUAPIPreimageCache { key in
            try? await IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main).lookupBy(rawHash: key)
        },
        cache: ChatResourceDiskCache(
            directory: URL.cachesDirectory.appending(path: "ChatResources", directoryHint: .isDirectory),
            maxBytes: 64 * 1_024 * 1_024
        )
    )

    /// The memory that the decoded pictures of the cache can take: 64 MiB.
    static let decodedBytesLimit = 64 * 1_024 * 1_024

    static func decodedBytes(of image: UIImage) -> Int {
        guard let bitmap = image.cgImage else { return 0 }
        return bitmap.bytesPerRow * bitmap.height
    }

    /// The longest side of a decoded picture, in pixels. A larger picture is scaled down while it decodes.
    static let maxDecodedPixelSize = 2_048

    /// Decodes the picture into a bitmap here, away from the main thread (P7.1, B4). `UIImage(data:)` decodes
    /// only when the screen first draws the picture, and the screen draws on the main thread. A picture of 1 MiB
    /// then stops the main thread for longer than one frame.
    ///
    /// A picture within `maxPixelSize` decodes at its size. A larger picture is scaled down while it decodes, so
    /// that the cache of bitmaps holds no picture larger than the screen needs.
    static func decodedImage(from data: Data, maxPixelSize: Int = maxDecodedPixelSize) -> UIImage? {
        guard let source = CGImageSourceCreateWithData(
            data as CFData,
            [kCGImageSourceShouldCache: false] as CFDictionary
        )
        else {
            return nil
        }

        let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        let width = properties?[kCGImagePropertyPixelWidth] as? Int ?? 0
        let height = properties?[kCGImagePropertyPixelHeight] as? Int ?? 0
        if max(width, height) <= maxPixelSize, let image = UIImage(data: data) {
            return image.preparingForDisplay() ?? image
        }

        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixelSize
        ] as CFDictionary
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else {
            return nil
        }

        return UIImage(cgImage: image)
    }

    static let fetchOverNetwork: Fetch = { url in
        let (data, response) = try await URLSession.shared.data(from: url)
        guard let http = response as? HTTPURLResponse, (200 ..< 300).contains(http.statusCode) else {
            throw URLError(.badServerResponse)
        }
        return data
    }
}

private extension ChatResourceResolver {
    func cachedData(for resource: ChatResource) async -> Data? {
        let key = resource.cacheKey
        if let data = await cache.data(for: key) {
            return data
        }

        let fetched: Data? =
            switch resource {
            case let .preimage(key):
                await preimages.lookup(key: key)
            case let .address(url):
                try? await fetch(url)
            case let .inline(_, data):
                data
            }

        if let fetched {
            await cache.store(fetched, for: key)
        }
        return fetched
    }
}

private extension ChatResource {
    var cacheKey: String {
        switch self {
        case let .inline(mimeType, data):
            "inline:\(mimeType):\(SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined())"
        case let .preimage(key):
            "preimage:\(key.base64EncodedString())"
        case let .address(url):
            "address:\(url.absoluteString)"
        }
    }
}
