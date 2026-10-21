import Foundation
import AsyncExtensions
import Products
@testable import polkadot_app

final class StubDotNsResolver: DotNsResolverProtocol {
    var contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")
    /// Content hash returned per name; an absent name reads as `0x00`.
    var contentHashes: [String: String] = [:]
    private(set) var resolvedNames: [String] = []

    func resolveToLocalURL(dotNsName: String) async throws -> URL {
        resolvedNames.append(dotNsName)
        return contentURL
    }

    func resolveContentHash(dotNsName: String) async throws -> String {
        contentHashes[dotNsName] ?? "0x00"
    }

    func resolveContent(dotNsName: String) async throws -> DotNsResolvedContent {
        resolvedNames.append(dotNsName)
        return try await DotNsResolvedContent(
            contentHash: resolveContentHash(dotNsName: dotNsName),
            localURL: contentURL
        )
    }

    func getMetadataEntry(dotNsName _: String, key _: String) async throws -> String? { nil }

    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func hasChatEntry(_: String) -> Bool { false }

    func clearCache() throws {}
}
