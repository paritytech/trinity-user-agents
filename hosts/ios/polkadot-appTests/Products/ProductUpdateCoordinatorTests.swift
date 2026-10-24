import Foundation
import FoundationExt
import Products
import Testing
import UIKit
@testable import polkadot_app

@MainActor
struct ProductUpdateCoordinatorTests {
    private static let productId = "shop.dot"
    private static let otherProductId = "other.dot"

    private let loader = StubProductContentLoader()
    private let coordinator: ProductUpdateCoordinator

    init() {
        coordinator = Self.makeCoordinator(loader: loader)
    }

    @Test func updateForAProductWithNoOpenSPAIsApplied() async {
        let update = Self.makeUpdate()
        coordinator.start()

        loader.simulate(update: update)
        await loader.waitForApplies(1)

        #expect(loader.appliedUpdates == [update])
    }

    @Test func updateForAnOpenProductWaitsUntilTheLastSPACloses() async {
        let first = StubSPAProductReloader()
        let second = StubSPAProductReloader()
        let firstId = coordinator.register(productId: Self.productId, reloader: first)
        let secondId = coordinator.register(productId: Self.productId, reloader: second)
        coordinator.start()

        let update = Self.makeUpdate()
        loader.simulate(update: update)
        loader.simulate(update: Self.makeUpdate(productId: Self.otherProductId))
        await loader.waitForApplies(1)

        coordinator.unregister(firstId)
        #expect(loader.appliedUpdates.map(\.productId) == [Self.otherProductId])

        coordinator.unregister(secondId)
        await loader.waitForApplies(2)

        #expect(loader.appliedUpdates.last == update)
        #expect(first.serveCount == 0)
        #expect(second.serveCount == 0)
    }

    @Test func refreshAppliesAPendingUpdateAndReloadsEveryOpenSPA() async {
        let first = StubSPAProductReloader()
        let second = StubSPAProductReloader()
        _ = coordinator.register(productId: Self.productId, reloader: first)
        _ = coordinator.register(productId: Self.productId, reloader: second)
        coordinator.start()

        loader.simulate(update: Self.makeUpdate())
        loader.simulate(update: Self.makeUpdate(productId: Self.otherProductId))
        await loader.waitForApplies(1)

        #expect(coordinator.applyPendingUpdate(of: Self.productId))
        await first.waitForServes(1)
        await second.waitForServes(1)

        #expect(loader.appliedUpdates.count == 2)
        #expect(!coordinator.applyPendingUpdate(of: Self.productId))
    }

    @Test func updatePublishedBeforeStartIsStillHandled() async {
        let update = Self.makeUpdate()
        let loader = StubProductContentLoader(pendingUpdates: [update])
        let coordinator = Self.makeCoordinator(loader: loader)

        coordinator.start()
        await loader.waitForApplies(1)

        #expect(loader.appliedUpdates == [update])
    }

    @Test func returningToTheForegroundRefreshesOpenProducts() async {
        _ = coordinator.register(productId: Self.productId, reloader: StubSPAProductReloader())
        coordinator.start()

        NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        await loader.waitForRefreshes(1)

        #expect(loader.refreshedIds == [Self.productId])
    }
}

private extension ProductUpdateCoordinatorTests {
    static func makeCoordinator(loader: StubProductContentLoader) -> ProductUpdateCoordinator {
        ProductUpdateCoordinator(
            contentLoader: loader,
            applicationStateStreams: ApplicationStateStreamFactory(),
            logger: Logger.shared
        )
    }

    static func makeUpdate(productId: ProductId = productId) -> ProductUpdate {
        let resolved = ResolvedProduct.legacy(id: productId, displayName: "Shop")
        let previous = ProductVersion(productId: productId, records: [:], contentHashes: [productId: "01"])
        let latest = ProductVersion(productId: productId, records: [:], contentHashes: [productId: "02"])
        let appContent = DotNsResolvedContent(contentHash: "02", localURL: URL(fileURLWithPath: "/content/02"))

        return ProductUpdate(
            previous: previous,
            latest: FetchedProductVersion(
                version: latest,
                resolved: resolved,
                appContent: appContent,
                contents: [productId: appContent]
            )
        )
    }
}
