import AsyncExtensions
import Foundation
import Kingfisher
import UIKit

struct PaymentAssetBrand: Equatable {
    let symbol: String
    let squareIcon: UIImage?
    let wideIcon: UIImage?
}

protocol PaymentAssetBrandingProviding: AnyObject {
    var current: PaymentAssetBrand { get }
    func stream() -> AnyAsyncSequence<PaymentAssetBrand>
}

protocol RemoteConfigObserving: AnyObject {
    func remoteConfigStream() -> AnyAsyncSequence<RemoteAppConfig>
}

protocol RemoteImageLoading: Sendable {
    func loadImage(from url: URL) async throws -> UIImage
}

final class PaymentAssetBranding: PaymentAssetBrandingProviding {
    static let shared = PaymentAssetBranding()

    private let subject: AsyncCurrentValueSubject<PaymentAssetBrand>
    private let configObserver: RemoteConfigObserving
    private let imageLoader: RemoteImageLoading
    private let fallbackSymbol: String
    private let logger: LoggerProtocol
    private var observation: Task<Void, Never>?

    init(
        configObserver: RemoteConfigObserving = FirebaseFacade.shared,
        imageLoader: RemoteImageLoading = KingfisherRemoteImageLoader(),
        fallbackSymbol: String = AppConfig.Brand.cashSymbol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.configObserver = configObserver
        self.imageLoader = imageLoader
        self.fallbackSymbol = fallbackSymbol
        self.logger = logger
        subject = AsyncCurrentValueSubject(PaymentAssetBrand(symbol: fallbackSymbol, squareIcon: nil, wideIcon: nil))
    }

    deinit {
        observation?.cancel()
    }

    var current: PaymentAssetBrand {
        subject.value
    }

    func stream() -> AnyAsyncSequence<PaymentAssetBrand> {
        subject.eraseToAnyAsyncSequence()
    }

    func start() {
        guard observation == nil else { return }

        observation = Task { [weak self] in
            guard let stream = self?.configObserver.remoteConfigStream() else { return }

            do {
                for try await config in stream {
                    guard let self, !Task.isCancelled else { return }
                    await apply(config.paymentAsset)
                }
            } catch {
                self?.logger.error("Payment asset branding stopped following remote config: \(error)")
            }
        }
    }
}

private extension PaymentAssetBranding {
    func apply(_ config: PaymentAssetConfig?) async {
        let symbol = config?.symbol ?? fallbackSymbol
        PaymentAssetSymbol.update(symbol)

        subject.send(PaymentAssetBrand(symbol: symbol, squareIcon: current.squareIcon, wideIcon: current.wideIcon))

        async let square = load(config?.squareIconURL)
        async let wide = load(config?.wideIconURL)
        let icons = await (square: square, wide: wide)

        guard !Task.isCancelled else { return }
        subject.send(PaymentAssetBrand(symbol: symbol, squareIcon: icons.square, wideIcon: icons.wide))
    }

    func load(_ url: URL?) async -> UIImage? {
        guard let url else { return nil }

        do {
            return try await imageLoader.loadImage(from: url)
        } catch {
            logger.error("Payment asset logo unavailable at \(url): \(error)")
            return nil
        }
    }
}

final class KingfisherRemoteImageLoader: RemoteImageLoading {
    private let optionsFactory: ImageProcessingOptionsProducing

    init(optionsFactory: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory()) {
        self.optionsFactory = optionsFactory
    }

    func loadImage(from url: URL) async throws -> UIImage {
        do {
            return try await retrieve(url, options: options + [.forceRefresh])
        } catch {
            guard let cached = try? await retrieve(url, options: options + [.onlyFromCache]) else {
                throw error
            }

            return cached
        }
    }

    private func retrieve(_ url: URL, options: KingfisherOptionsInfo) async throws -> UIImage {
        try await withCheckedThrowingContinuation { continuation in
            KingfisherManager.shared.retrieveImage(with: url, options: options) { result in
                switch result {
                case let .success(retrieved) where retrieved.image.hasPixels:
                    continuation.resume(returning: retrieved.image)
                case .success:
                    continuation.resume(throwing: RemoteImageError.emptyImage)
                case let .failure(error):
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    private var options: KingfisherOptionsInfo {
        optionsFactory.options(for: .originalImage, animated: false) + [.diskCacheExpiration(.never)]
    }
}

enum RemoteImageError: Error {
    case emptyImage
}

private extension UIImage {
    var hasPixels: Bool {
        size.width > 0 && size.height > 0
    }
}
