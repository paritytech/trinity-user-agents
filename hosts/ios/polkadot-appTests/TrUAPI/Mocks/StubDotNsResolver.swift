import Foundation
import AsyncExtensions
import Products
@testable import polkadot_app

final class StubDotNsResolver: DotNsResolverProtocol {
    let contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")
    private(set) var resolvedNames: [String] = []

    func resolveToLocalURL(dotNsName: String) async throws -> URL {
        resolvedNames.append(dotNsName)
        return contentURL
    }

    func resolveContent(dotNsName: String) async throws -> DotNsResolvedContent {
        try await DotNsResolvedContent(contentHash: "00", localURL: resolveToLocalURL(dotNsName: dotNsName))
    }

    func stageContent(dotNsName: String) async throws -> DotNsResolvedContent {
        try await resolveContent(dotNsName: dotNsName)
    }

    func getMetadataEntry(dotNsName _: String, key _: String) async throws -> String? { nil }

    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func hasChatEntry(_: String) -> Bool { false }

    func clearCache() throws {}
}
