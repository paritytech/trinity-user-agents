import AsyncExtensions
import Foundation
import Testing
import UIKit

@testable import polkadot_app

@Suite("Payment asset branding")
@MainActor
struct PaymentAssetBrandingTests {
    private let squareURL = URL(string: "https://cdn.example.com/square.svg")!
    private let wideURL = URL(string: "https://cdn.example.com/wide.svg")!

    @Test("Starts from the bundled brand before any config arrives")
    func initialBrandIsFallback() {
        let branding = makeBranding(configs: StubConfigStream(), loader: StubImageLoader(images: [:]))

        #expect(branding.current == PaymentAssetBrand(symbol: "CASH", squareIcon: nil, wideIcon: nil))
    }

    @Test("Applies the remote symbol at once and the logos when fetched")
    func appliesSymbolThenLogos() async throws {
        let square = UIImage.solid(.red)
        let wide = UIImage.solid(.blue)
        let configs = StubConfigStream()
        let loader = StubImageLoader(images: [squareURL: square, wideURL: wide])
        let branding = makeBranding(configs: configs, loader: loader)
        var iterator = branding.stream().makeAsyncIterator()
        _ = try await iterator.next()

        branding.start()
        configs.send(remoteConfig(symbol: "USD", square: squareURL, wide: wideURL))

        let symbolOnly = try await iterator.next()
        #expect(symbolOnly == PaymentAssetBrand(symbol: "USD", squareIcon: nil, wideIcon: nil))

        let withLogos = try #require(try await iterator.next())
        #expect(withLogos.symbol == "USD")
        #expect(withLogos.squareIcon === square)
        #expect(withLogos.wideIcon === wide)
        #expect(loader.requestedURLs.sorted { $0.absoluteString < $1.absoluteString } == [squareURL, wideURL])
    }

    @Test("A logo that fails to load leaves that slot on the bundled mark")
    func failedLogoFallsBack() async throws {
        let wide = UIImage.solid(.blue)
        let configs = StubConfigStream()
        let loader = StubImageLoader(images: [wideURL: wide])
        let branding = makeBranding(configs: configs, loader: loader)
        var iterator = branding.stream().makeAsyncIterator()
        _ = try await iterator.next()

        branding.start()
        configs.send(remoteConfig(symbol: nil, square: squareURL, wide: wideURL))

        _ = try await iterator.next()
        let brand = try #require(try await iterator.next())
        #expect(brand.symbol == "CASH")
        #expect(brand.squareIcon == nil)
        #expect(brand.wideIcon === wide)
    }

    @Test("A config without the object keeps the fallback symbol")
    func missingObjectKeepsFallback() async throws {
        let configs = StubConfigStream()
        let loader = StubImageLoader(images: [:])
        let branding = makeBranding(configs: configs, loader: loader)
        var iterator = branding.stream().makeAsyncIterator()
        _ = try await iterator.next()

        branding.start()
        configs.send(remoteConfig(symbol: nil, square: nil, wide: nil, published: false))

        let brand = try #require(try await iterator.next())
        #expect(brand.symbol == "CASH")
        #expect(loader.requestedURLs.isEmpty)
    }
}

private extension PaymentAssetBrandingTests {
    func makeBranding(configs: StubConfigStream, loader: StubImageLoader) -> PaymentAssetBranding {
        PaymentAssetBranding(
            configObserver: configs,
            imageLoader: loader,
            fallbackSymbol: "CASH",
            logger: StubLogger()
        )
    }

    func remoteConfig(symbol: String?, square: URL?, wide: URL?, published: Bool = true) -> RemoteAppConfig {
        RemoteAppConfig(
            identityBackendUrl: nil,
            ipfsGatewayUrl: nil,
            dotNsResolver: nil,
            dotNsNameRegistry: nil,
            coinageInstanceId: nil,
            fundingUrl: nil,
            offrampUrl: nil,
            accountDataStoreContract: nil,
            paymentAsset: published
                ? PaymentAssetConfig(symbol: symbol, squareIconURL: square, wideIconURL: wide)
                : nil,
            appSharingUrl: nil
        )
    }
}

private final class StubConfigStream: RemoteConfigObserving {
    private let subject = AsyncCurrentValueSubject<RemoteAppConfig?>(nil)

    func remoteConfigStream() -> AnyAsyncSequence<RemoteAppConfig> {
        subject.compacted().eraseToAnyAsyncSequence()
    }

    func send(_ config: RemoteAppConfig) {
        subject.send(config)
    }
}

private final class StubImageLoader: RemoteImageLoading, @unchecked Sendable {
    struct Missing: Error {}

    private let images: [URL: UIImage]
    private let lock = NSLock()
    private var requested: [URL] = []

    init(images: [URL: UIImage]) {
        self.images = images
    }

    var requestedURLs: [URL] {
        lock.withLock { requested }
    }

    func loadImage(from url: URL) async throws -> UIImage {
        lock.withLock { requested.append(url) }

        guard let image = images[url] else {
            throw Missing()
        }

        return image
    }
}

private extension UIImage {
    static func solid(_ color: UIColor) -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: 1, height: 1)).image { context in
            color.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
        }
    }
}
