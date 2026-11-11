import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct ProductArchiveWarmerTests {
    @Test(arguments: [
        ExecutableKindStrategy.fixed(.app),
        .fixed(.widget),
        .fixed(.worker),
        .withAppFallback(.widget)
    ])
    func missingPublishedExecutableDoesNotFetchTheBaseArchive(_ strategy: ExecutableKindStrategy) async throws {
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
        let warmer = ProductArchiveWarmer(
            products: StubProductResolver(alwaysResolvingTo: resolved),
            dotNsResolver: archives
        )

        await #expect(throws: ProductResolutionError.self) {
            try await warmer.warm(productId, serving: strategy)
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
            try await warmer.warm(ProductId("game.dot"), serving: .fixed(.app))
        }
        #expect(archives.resolvedNames.isEmpty)
    }

    @Test func noManifestAppFetchesItsExplicitBaseArchive() async throws {
        let productId = try ProductId("game.dot")
        let archives = StubDotNsResolver()
        let warmer = ProductArchiveWarmer(products: StubProductResolver(), dotNsResolver: archives)

        try await warmer.warm(productId, serving: .fixed(.app))

        #expect(archives.resolvedNames == [productId.rawValue])
    }

    /// A card that falls back to the app has to warm the archive it will open, not the widget's.
    @Test(arguments: [true, false])
    func fallbackWarmsTheWidgetWhenPublishedAndTheAppOtherwise(_ publishesWidget: Bool) async throws {
        let archives = StubDotNsResolver()
        let warmer = try ProductArchiveWarmer(
            products: StubProductResolver(alwaysResolvingTo: product(publishesWidget: publishesWidget)),
            dotNsResolver: archives
        )

        try await warmer.warm(ProductId("game.dot"), serving: .withAppFallback(.widget))

        #expect(archives.resolvedNames == [publishesWidget ? "widget.game.dot" : "app.game.dot"])
    }
}

private extension ProductArchiveWarmerTests {
    func product(publishesWidget: Bool) throws -> ResolvedProduct {
        let widget = try ProductExecutable.Widget(
            identifier: ProductId("widget.game.dot"),
            appVersion: .zero,
            description: nil,
            heights: [2],
            width: 1
        )

        return try ResolvedProduct(
            id: ProductId("game.dot"),
            displayName: "Game",
            description: nil,
            icon: nil,
            executables: ProductExecutables(
                app: ProductExecutable.App(identifier: ProductId("app.game.dot"), appVersion: .zero),
                widget: publishesWidget ? widget : nil,
                worker: nil
            ),
            hasManifest: true
        )
    }
}
