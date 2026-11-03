import Foundation
import Products
@testable import polkadot_app

final class StubDotNsResolver: DotNsResolverProtocol {
    let contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")
    private(set) var resolvedNames: [String] = []

    func resolveToLocalURL(dotNsName: String) async throws -> URL {
        resolvedNames.append(dotNsName)
        return contentURL
    }

    func clearCache() throws {}
}
