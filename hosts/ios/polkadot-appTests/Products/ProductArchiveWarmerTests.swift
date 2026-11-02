import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct ProductArchiveWarmerTests {
    @Test(arguments: [ExecutableKind.app, .widget, .worker])
    func missingPublishedExecutableDoesNotFetchTheBaseArchive(_ kind: ExecutableKind) async throws {
        let productId = try ProductId("game.dot")
        let resolved = ResolvedProduct(
            id: productId,
            displayName: "Game",
            description: nil,
            icon: nil,
            executables: ProductExecutables(app: nil, widget: nil, worker: nil),
            hasManifest: true
        )
        let archives = StubDotNsResolver()
        let warmer = ProductArchiveWarmer(products: StubProductResolver(alwaysResolvingTo: resolved), dotNsResolver: archives)

        await #expect(throws: ProductResolutionError.self) {
            try await warmer.warm(productId, serving: kind)
        }
        #expect(archives.resolvedNames.isEmpty)
    }

    @Test func resolutionFailureDoesNotFetchTheBaseArchive() async throws {
        let archives = StubDotNsResolver()
        let warmer = ProductArchiveWarmer(
            products: StubProductResolver(alwaysFailing: URLError(.timedOut)),
            dotNsResolver: archives
        )

        await #expect(throws: URLError.self) {
            try await warmer.warm(ProductId("game.dot"), serving: .app)
        }
        #expect(archives.resolvedNames.isEmpty)
    }

    @Test func noManifestAppFetchesItsExplicitBaseArchive() async throws {
        let productId = try ProductId("game.dot")
        let archives = StubDotNsResolver()
        let warmer = ProductArchiveWarmer(products: StubProductResolver(), dotNsResolver: archives)

        try await warmer.warm(productId, serving: .app)

        #expect(archives.resolvedNames == [productId.rawValue])
    }
}
