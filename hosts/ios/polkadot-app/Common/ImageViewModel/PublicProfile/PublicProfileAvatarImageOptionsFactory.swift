import ImageIO
import Kingfisher
import PolkadotUI
import enum TrUAPIHost.ImageFormat
import UniformTypeIdentifiers

final class PublicProfileAvatarImageOptionsFactory: ImageProcessingOptionsProducing {
    static let cache: ImageCache = {
        let cache = ImageCache(name: "public-profile-avatars")
        cache.diskStorage.config.sizeLimit = 100 * 1_024 * 1_024
        return cache
    }()

    private let format: ImageFormat
    private let targetCache: ImageCache
    private let base: ImageProcessingOptionsProducing

    init(
        format: ImageFormat,
        cache: ImageCache = PublicProfileAvatarImageOptionsFactory.cache,
        base: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory()
    ) {
        self.format = format
        targetCache = cache
        self.base = base
    }

    func options(for settings: ImageViewModelSettings, animated: Bool) -> KingfisherOptionsInfo {
        let options = base.options(for: settings, animated: animated)
        let processor = KingfisherParsedOptionsInfo(options).processor
        return options + [
            .processor(PublicProfileAvatarImageProcessor(format: format, processor: processor)),
            .cacheSerializer(FormatIndicatedCacheSerializer.png),
            .targetCache(targetCache),
            .cacheOriginalImage,
            .memoryCacheExpiration(.never),
            .diskCacheExpiration(.never),
            .diskCacheAccessExtendingExpiration(.expirationTime(.never))
        ]
    }
}

private struct PublicProfileAvatarImageProcessor: ImageProcessor {
    let format: ImageFormat
    let processor: any ImageProcessor

    var identifier: String { "public-profile:\(format)|\(processor.identifier)" }

    func process(item: ImageProcessItem, options: KingfisherParsedOptionsInfo) -> KFCrossPlatformImage? {
        let expectedType: UTType =
            switch format {
            case .png: .png
            case .jpeg: .jpeg
            case .webp: .webP
            case .gif: .gif
            }
        guard case let .data(bytes) = item,
              let source = CGImageSourceCreateWithData(bytes as CFData, nil),
              let type = CGImageSourceGetType(source), type as String == expectedType.identifier else { return nil }
        return processor.process(item: item, options: options)
    }
}
