import UIKit
import PolkadotUI
import BlurHash

final class BlurHashMediaPreviewProvider: ChatMessageMediaPreviewProviding {
    private static let maximumCachedPreviewCount = 100

    private static let imageCache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.countLimit = maximumCachedPreviewCount
        return cache
    }()

    private let blurHash: BlurHash?
    private let dataExistencePredicate: () -> Bool
    private let mediaProvider: any ChatMessageMediaPreviewProviding

    init(
        thumbnail: Data?,
        dataExistencePredicate: @escaping () -> Bool,
        mediaProvider: any ChatMessageMediaPreviewProviding
    ) {
        blurHash = thumbnail.flatMap { BlurHash(rawValue: $0) }
        self.dataExistencePredicate = dataExistencePredicate
        self.mediaProvider = mediaProvider
    }

    var identifier: String {
        mediaProvider.identifier + ":" + (blurHash?.value ?? "no-blur-hash")
    }

    @MainActor
    func providePreview(for imageView: UIImageView, size: CGSize?) {
        if !dataExistencePredicate(), let blurHash, let size {
            let previewSize = previewSize(for: size)
            let cacheKey = "\(blurHash.value):\(Int(previewSize.width))x\(Int(previewSize.height))" as NSString
            if let cachedImage = Self.imageCache.object(forKey: cacheKey) {
                imageView.image = cachedImage
            } else if let image = UIImage(blurHash: blurHash, size: previewSize) {
                Self.imageCache.setObject(image, forKey: cacheKey)
                imageView.image = image
            }
        }
        mediaProvider.providePreview(for: imageView, size: size)
    }
}

private extension BlurHashMediaPreviewProvider {
    func previewSize(for targetSize: CGSize) -> CGSize {
        let maximumSide = BlurHashConfiguration.decodingPreviewMaximumSide
        let scale = min(1, maximumSide / max(targetSize.width, targetSize.height))
        return CGSize(
            width: max(1, targetSize.width * scale),
            height: max(1, targetSize.height * scale)
        )
    }
}
