import Foundation
import AsyncExtensions
import PolkadotUI
import Products
import Testing
@testable import polkadot_app

/// A chat card names its images the way a Pocket card does, so the chat reads
/// them from the same places: the product's worker archive, which serves the
/// chat, and the Bulletin gateway.
struct ChatCardImagesTests {
    @Test
    func readsAnArchiveImageOutOfTheProductsWorkerArchive() async throws {
        let root = try makeArchive(files: ["art/prize.png": "png"])
        let resolver = RecordingResolver(root: root)
        let images = ProductBotFactory.chatImages(
            for: gameProduct(),
            dotNsResolver: resolver,
            ipfsUrl: { _ in nil }
        )

        let url = try #require(await images(.archive(path: "art/prize.png")))

        #expect(url == root.appending(path: "art/prize.png"))
        #expect(resolver.asked() == ["worker.game.paseo"])
    }

    @Test
    func readsABulletinImageThroughTheGateway() async throws {
        let images = ProductBotFactory.chatImages(
            for: gameProduct(),
            dotNsResolver: RecordingResolver(root: URL(fileURLWithPath: "/tmp/none")),
            ipfsUrl: { URL(string: "https://gateway.invalid/ipfs/\($0)") }
        )

        let url = try #require(await images(.bulletin(cid: "bafyprize")))

        #expect(url.absoluteString == "https://gateway.invalid/ipfs/bafyprize")
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
                modalities: []
            )
        ),
        hasManifest: true
    )
}

private func makeArchive(files: [String: String]) throws -> URL {
    let root = URL.temporaryDirectory
        .appending(path: "chat-images-\(UUID().uuidString)")
        .appending(path: "content")
    for (path, contents) in files {
        let file = root.appending(path: path)
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(contents.utf8).write(to: file)
    }
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
