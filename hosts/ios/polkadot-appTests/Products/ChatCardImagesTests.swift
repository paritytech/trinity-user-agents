import Foundation
import AsyncExtensions
import PolkadotUI
import Products
import Testing
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

/// A chat card names its images the way a Pocket card does, so the bot the
/// factory builds hands its decoder a loader over the same places: the
/// product's worker archive, which serves the chat, and the Bulletin gateway.
struct ChatCardImagesTests {
    @Test
    func readsAnArchiveImageOutOfTheProductsWorkerArchive() async throws {
        let root = try makePictureDirectory(named: "art/prize.png")
        defer { try? FileManager.default.removeItem(at: root) }
        let resolver = RecordingResolver(root: root)
        let images = try chatImages(dotNsResolver: resolver, gateway: root)

        let image = await images(.archive(path: "art/prize.png"))

        #expect(image?.size == pictureSize)
        #expect(resolver.asked() == ["worker.game.paseo"])
    }

    @Test
    func readsABulletinImageThroughTheGateway() async throws {
        let gateway = try makePictureDirectory(named: "bafyprize")
        defer { try? FileManager.default.removeItem(at: gateway) }
        let resolver = RecordingResolver(root: URL(fileURLWithPath: "/tmp/none"))
        let images = try chatImages(dotNsResolver: resolver, gateway: gateway)

        let image = await images(.bulletin(cid: "bafyprize"))

        #expect(image?.size == pictureSize)
        #expect(resolver.asked().isEmpty)
    }

    /// The resolver the chat decoder draws with, reached the way the app
    /// reaches it: through the bot the factory builds for the product.
    private func chatImages(
        dotNsResolver: any DotNsResolverProtocol,
        gateway: URL
    ) throws -> WidgetImageResolver {
        let factory = ProductBotFactory(
            productFileProvider: NoScripts(),
            runtimeProvider: NoRuntime(),
            workers: { nil },
            workerManager: NoWorkers(),
            productImages: ProductImageSources(dotNsResolver: dotNsResolver, ipfsGatewayBaseUrl: gateway)
        )
        let bot = try #require(factory.create(resolved: gameProduct()))
        let decoder = try #require(bot.customDecoders.first as? ProductMessageDecoder)

        return decoder.resolveImage
    }
}

private struct Unavailable: Error {}

private struct NoScripts: ChatProductFileProviding {
    func load(for _: ProductId, relativePath _: String) -> Data? { nil }
    func manualScriptEntryPath(productId _: ProductId) -> String? { nil }
}

private final class NoRuntime: TrUAPIHostRuntimeProviding {
    func sharedRuntime() throws -> TrUAPIHostRuntime { throw Unavailable() }
    @MainActor func setPresentationView(_: ControllerBackedProtocol) {}
    func attach(workerManager _: any TrUAPIWorkerManaging) {}
}

private struct NoWorkers: ProductWorkerManaging {
    func acquire(productId _: ProductId) async -> ProductWorkerLease {
        ProductWorkerLease(token: ProductWorkerToken {}, result: .failure(Unavailable()))
    }
}

private func gameProduct() -> ResolvedProduct {
    ResolvedProduct(
        id: "game.paseo",
        displayName: "Game",
        description: nil,
        icon: nil,
        executables: ProductExecutables(
            app: nil,
            widget: nil,
            worker: ProductExecutable.Worker(
                identifier: "worker.game.paseo",
                appVersion: .zero,
                entrypoint: "index.js",
                modalities: [.chat]
            )
        ),
        hasManifest: true
    )
}

private let pictureSize = CGSize(width: 3, height: 2)

private func makePictureDirectory(named path: String) throws -> URL {
    let root = URL.temporaryDirectory.appending(path: "chat-images-\(UUID().uuidString)")
    let file = root.appending(path: path)
    try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
    try FileManager.default.moveItem(at: writeTestPicture(size: pictureSize), to: file)
    return root
}

private final class RecordingResolver: DotNsResolverProtocol, @unchecked Sendable {
    private let root: URL
    private let lock = NSLock()
    private var names: [String] = []

    init(root: URL) {
        self.root = root
    }

    func asked() -> [String] {
        lock.withLock { names }
    }

    func resolveToLocalURL(dotNsName: String) async throws -> URL {
        lock.withLock { names.append(dotNsName) }
        return root
    }

    func getMetadataEntry(dotNsName _: String, key _: String) async throws -> String? { nil }
    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func clearCache() throws {}
}
