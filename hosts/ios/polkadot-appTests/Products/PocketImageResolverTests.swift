import Foundation
import AsyncExtensions
import PolkadotUI
import Products
import Testing
@testable import polkadot_app

/// Turns an image source inside a face into something the image loader can
/// fetch. A source that cannot be resolved answers nil: the rest of the face
/// still draws, with a hole where the image would be.
///
/// What a path is allowed to name is the archive's rule, and
/// ``ProductWorkerArchiveTests`` proves it. What is left here is which archive
/// gets asked, and what a source that is not a file becomes.
struct PocketImageResolverTests {
    @Test
    func resolvesAnArchiveImageToAFileInTheWorkersArchive() async throws {
        let root = try makeArchive(files: ["art/badge.png": "png"])
        let resolver = PocketImageResolver(
            contentId: { "worker.game.paseo" },
            archive: ProductWorkerArchive(dotNsResolver: StubResolver(root: root), cachedRoot: { _ in nil }),
            ipfsUrl: { _ in nil }
        )

        let url = try #require(await resolver.resolve(.archive(path: "art/badge.png")))

        #expect(url.isFileURL)
        #expect(try Data(contentsOf: url) == Data("png".utf8))
    }

    /// Every image node in a face asks again each time it appears, and going to
    /// the chain for each one costs a contract read the archive on disk already
    /// answers. Offline that read fails outright, and a card drawn from a face
    /// it kept would lose every image in it.
    @Test
    func readsAnArchiveAlreadyOnDiskWithoutAskingTheChain() async throws {
        let root = try makeArchive(files: ["art/badge.png": "png"])
        let resolver = PocketImageResolver(
            contentId: { "worker.game.paseo" },
            archive: ProductWorkerArchive(dotNsResolver: FailingResolver(), cachedRoot: { _ in root }),
            ipfsUrl: { _ in nil }
        )

        let url = try #require(await resolver.resolve(.archive(path: "art/badge.png")))

        #expect(try Data(contentsOf: url) == Data("png".utf8))
    }

    /// Nothing on disk yet is the first draw of a card whose product has never
    /// been fetched, which is exactly when the chain read is worth paying.
    @Test
    func fetchesTheArchiveWhenNothingIsOnDiskYet() async throws {
        let root = try makeArchive(files: ["art/badge.png": "png"])
        let resolver = PocketImageResolver(
            contentId: { "worker.game.paseo" },
            archive: ProductWorkerArchive(dotNsResolver: StubResolver(root: root), cachedRoot: { _ in nil }),
            ipfsUrl: { _ in nil }
        )

        #expect(await resolver.resolve(.archive(path: "art/badge.png")) != nil)
    }

    @Test
    func resolvesABulletinImageToItsGatewayAddress() async throws {
        let resolver = PocketImageResolver(
            contentId: { "worker.game.paseo" },
            archive: ProductWorkerArchive(
                dotNsResolver: StubResolver(root: URL(fileURLWithPath: "/tmp/none")),
                cachedRoot: { _ in nil }
            ),
            ipfsUrl: { URL(string: "https://gateway.invalid/ipfs/\($0)") }
        )

        let url = try #require(await resolver.resolve(.bulletin(cid: "bafyimage")))

        #expect(url.absoluteString == "https://gateway.invalid/ipfs/bafyimage")
    }

    /// A cold start with no network reaches no manifest, so the name the
    /// product publishes its worker archive under cannot be read. The subname
    /// dotNS fixes by convention needs no read at all, so a card whose archive
    /// is already unpacked draws its images out of it rather than losing them
    /// to a manifest read that cannot land.
    @Test
    func drawsFromTheArchiveOnDiskWhenTheManifestCannotBeRead() async throws {
        let root = try makeArchive(files: ["art/badge.png": "png"])
        let onDisk: @Sendable (ProductId) -> URL? = { $0 == "worker.game.paseo" ? root : nil }
        let name = PocketWorkerArchiveName(productId: "game.paseo", published: { nil }, onDisk: onDisk)
        let resolver = PocketImageResolver(
            contentId: { await name.resolve() },
            archive: ProductWorkerArchive(dotNsResolver: FailingResolver(), cachedRoot: onDisk),
            ipfsUrl: { _ in nil }
        )

        let url = try #require(await resolver.resolve(.archive(path: "art/badge.png")))

        #expect(try Data(contentsOf: url) == Data("png".utf8))
    }

    /// Nothing unpacked under the conventional subname is a worker this host
    /// has never fetched, and only the manifest can say where that one lives.
    @Test
    func asksTheManifestForAWorkerArchiveThatIsNotOnDisk() async {
        let name = PocketWorkerArchiveName(
            productId: "game.paseo",
            published: { "worker.elsewhere.paseo" },
            onDisk: { _ in nil }
        )

        #expect(await name.resolve() == "worker.elsewhere.paseo")
    }

    /// An archive that cannot be fetched leaves the rest of the face drawable.
    @Test
    func answersNoUrlWhenTheArchiveCannotBeRead() async {
        let resolver = PocketImageResolver(
            contentId: { "worker.game.paseo" },
            archive: ProductWorkerArchive(dotNsResolver: FailingResolver(), cachedRoot: { _ in nil }),
            ipfsUrl: { _ in nil }
        )

        #expect(await resolver.resolve(.archive(path: "art/badge.png")) == nil)
    }
}

// MARK: - Fixtures

private func makeArchive(files: [String: String]) throws -> URL {
    let root = URL.temporaryDirectory
        .appending(path: "pocket-images-\(UUID().uuidString)")
        .appending(path: "content")
    for (path, contents) in files {
        let file = root.appending(path: path)
        try FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data(contents.utf8).write(to: file)
    }
    return root
}

private struct StubResolver: DotNsResolverProtocol {
    let root: URL

    func resolveToLocalURL(dotNsName _: String) async throws -> URL { root }
    func getMetadataEntry(dotNsName _: String, key _: String) async throws -> String? { nil }
    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func clearCache() throws {}
}

private struct FailingResolver: DotNsResolverProtocol {
    func resolveToLocalURL(dotNsName: String) async throws -> URL {
        throw DotNsResolverError.resolutionFailed(dotNsName)
    }

    func getMetadataEntry(dotNsName _: String, key _: String) async throws -> String? { nil }
    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func clearCache() throws {}
}
