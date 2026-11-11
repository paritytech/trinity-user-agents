import ImageIO
import Kingfisher
import PolkadotUI
import enum TrUAPIHost.ImageFormat
import enum TrUAPIHost.ImageReference
import UIKit
import UniformTypeIdentifiers

@MainActor
final class CIDImageLoader {
    private let runtimeProvider: TrUAPIHostRuntimeProviding

    nonisolated init(runtimeProvider: TrUAPIHostRuntimeProviding) {
        self.runtimeProvider = runtimeProvider
    }

    func load(
        _ reference: ImageReference,
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        let format =
            switch reference {
            case let .bulletin(_, format): format
            }
        var options = ImageProcessingOptionsFactory().options(for: settings, animated: animated)
        let processor = KingfisherParsedOptionsInfo(options).processor
        options += [
            .processor(CIDImageProcessor(format: format, processor: processor)),
            .cacheSerializer(FormatIndicatedCacheSerializer.png),
            .memoryCacheExpiration(.never),
            .diskCacheExpiration(.never),
            .diskCacheAccessExtendingExpiration(.expirationTime(.never)),
            .keepCurrentImageWhileLoading
        ]
        let provider = CIDImageDataProvider(reference: reference, runtimeProvider: runtimeProvider)
        imageView.kf.setImage(with: provider, options: options) { result in
            switch result {
            case .success:
                completion?(true)
            case .failure(.requestError(reason: .dataProviderCancelled)):
                break
            case let .failure(error):
                if !error.isTaskCancelled, !error.isNotCurrentTask {
                    completion?(false)
                }
            }
        }
    }

    func cancel(on imageView: UIImageView) {
        imageView.kf.cancelDownloadTask()
    }
}

private struct CIDImageDataProvider: ImageDataProvider {
    let reference: ImageReference
    let runtimeProvider: TrUAPIHostRuntimeProviding

    var cacheKey: String {
        switch reference {
        case let .bulletin(cid, _): cid
        }
    }

    func data() async throws -> Data {
        try Task.checkCancellation()
        let runtime = try runtimeProvider.sharedRuntime()
        try Task.checkCancellation()
        let bytes = try await runtime.loadImage(reference: reference).bytes
        try Task.checkCancellation()
        return bytes
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
