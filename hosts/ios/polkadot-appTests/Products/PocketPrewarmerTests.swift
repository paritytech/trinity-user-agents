import Foundation
import AsyncExtensions
import Products
import Testing
@testable import polkadot_app

/// Pressing a card opens the product's widget, so the archive the widget is
/// served from is fetched ahead of the press. Only for host-placed cards: they
/// are few and fixed, where a Pocket full of added cards would turn one visit
/// to the tab into a fetch per product.
@MainActor
struct PocketPrewarmerTests {
    /// A pressed card opens the widget, so warming the app archive fetches
    /// something the press never loads and leaves the press slow anyway.
    @Test
    func warmsTheArchiveAPressedCardOpens() async {
        let archives = RecordingArchives()
        let prewarmer = PocketPrewarmer(products: resolverPublishingWidgets(), dotNsResolver: archives)

        await prewarmer.warm([humanity])

        #expect(archives.warmed == ["widget.peopl.testnet"])
    }

    /// The bound that makes this safe: a user may hold many added cards, and
    /// warming each one would fetch an archive per product on every visit.
    @Test
    func leavesACardTheUserAddedAlone() async {
        let archives = RecordingArchives()
        let prewarmer = PocketPrewarmer(products: resolverPublishingWidgets(), dotNsResolver: archives)

        await prewarmer.warm([loyalty])

        #expect(archives.warmed.isEmpty)
    }

    /// The tab is visited over and over, and two host-placed cards may sit on
    /// one product; neither should cost a second fetch.
    @Test
    func warmsAProductOnlyOnce() async {
        let archives = RecordingArchives()
        let prewarmer = PocketPrewarmer(products: resolverPublishingWidgets(), dotNsResolver: archives)

        await prewarmer.warm([humanity, humanity])
        await prewarmer.warm([humanity])

        #expect(archives.warmed == ["widget.peopl.testnet"])
    }

    /// A warm that failed left nothing on disk, so the next visit tries again
    /// rather than leaving the card slow for the rest of the run.
    @Test
    func triesAgainAfterAFailedWarm() async {
        let archives = RecordingArchives(failing: true)
        let prewarmer = PocketPrewarmer(products: resolverPublishingWidgets(), dotNsResolver: archives)

        await prewarmer.warm([humanity])
        archives.failing = false
        await prewarmer.warm([humanity])

        #expect(archives.warmed == ["widget.peopl.testnet", "widget.peopl.testnet"])
    }
}

// MARK: - Fixtures

private let humanity = PocketCardViewModel(
    key: PocketCardKey(productId: "peopl.testnet", cardId: PocketCardId(value: "humanity")),
    title: "Humanity",
    privileged: true,
    face: nil
)

private let loyalty = PocketCardViewModel(
    key: PocketCardKey(productId: "game.testnet", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false,
    face: nil
)

/// Every product here publishes both an app and a widget under subnames of its own, which is what
/// makes the archive a warm reaches for something the test can name.
private func resolverPublishingWidgets() -> StubProductResolver {
    StubProductResolver { productId in
        ResolvedProduct(
            id: productId,
            displayName: productId,
            description: nil,
            icon: nil,
            executables: ProductExecutables(
                app: ProductExecutable.App(identifier: "app.\(productId)", appVersion: .zero),
                widget: ProductExecutable.Widget(
                    identifier: "widget.\(productId)",
                    appVersion: .zero,
                    description: nil,
                    heights: [2],
                    width: 4
                ),
                worker: nil
            ),
            hasManifest: true
        )
    }
}

private final class RecordingArchives: DotNsResolverProtocol, @unchecked Sendable {
    private(set) var warmed: [String] = []
    var failing: Bool

    init(failing: Bool = false) {
        self.failing = failing
    }

    func resolveToLocalURL(dotNsName: String) async throws -> URL {
        warmed.append(dotNsName)
        if failing { throw DotNsResolverError.resolutionFailed(dotNsName) }
        return URL(fileURLWithPath: "/tmp/\(dotNsName)")
    }

    func getMetadataEntry(dotNsName _: String, key _: String) async throws -> String? { nil }
    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func clearCache() throws {}
}
