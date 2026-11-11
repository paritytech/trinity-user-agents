import ImageIO
import Kingfisher
import PolkadotUI
import enum TrUAPIHost.ImageFormat
import UniformTypeIdentifiers

final class CIDImageOptionsFactory: ImageProcessingOptionsProducing {
    private let format: ImageFormat?
    private let base: ImageProcessingOptionsProducing

    init(
        format: ImageFormat?,
        base: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory()
    ) {
        self.format = format
        self.base = base
    }

    func options(for settings: ImageViewModelSettings, animated: Bool) -> KingfisherOptionsInfo {
        let options = base.options(for: settings, animated: animated)
        let processor = KingfisherParsedOptionsInfo(options).processor
        return options + [
            .processor(CIDImageProcessor(format: format, processor: processor)),
            .cacheSerializer(FormatIndicatedCacheSerializer.png),
            .cacheOriginalImage,
            .memoryCacheExpiration(.never),
            .diskCacheExpiration(.never),
            .diskCacheAccessExtendingExpiration(.expirationTime(.never))
        ]
    }
}

private struct CIDImageProcessor: ImageProcessor {
    let format: ImageFormat?
    let processor: any ImageProcessor

    var identifier: String { "cid:\(String(describing: format))|\(processor.identifier)" }

    func process(item: ImageProcessItem, options: KingfisherParsedOptionsInfo) -> KFCrossPlatformImage? {
        guard let format else { return processor.process(item: item, options: options) }
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
