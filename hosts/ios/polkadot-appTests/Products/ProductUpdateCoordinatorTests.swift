import Foundation
import FoundationExt
import Products
import Testing
import UIKit
@testable import polkadot_app

@MainActor
struct ProductUpdateCoordinatorTests {
    private static let productId = "shop.dot"
    private static let appId = "app.shop.dot"
    private static let workerId = "worker.shop.dot"
    private static let otherAppId = "app.other.dot"

    private let loader = StubProductContentLoader()
    private let coordinator: ProductUpdateCoordinator
    private let first = StubSPAProductReloader()
    private let second = StubSPAProductReloader()

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

    @Test func updateForAnOpenProductStaysPendingWhileAnotherSPAShowsIt() async {
        let firstId = coordinator.register(productId: Self.productId, contentId: Self.appId, reloader: first)
        _ = coordinator.register(productId: Self.productId, contentId: Self.appId, reloader: second)
        coordinator.start()

        await deliver(Self.makeUpdate())

        coordinator.unregister(firstId)

        #expect(coordinator.applyPendingUpdate(of: Self.appId))
    }

    @Test func updateForAnOpenProductIsAppliedOnceTheLastSPACloses() async {
        let firstId = coordinator.register(productId: Self.productId, contentId: Self.appId, reloader: first)
        let secondId = coordinator.register(productId: Self.productId, contentId: Self.appId, reloader: second)
        coordinator.start()

        let update = Self.makeUpdate()
        await deliver(update)

        coordinator.unregister(firstId)
        coordinator.unregister(secondId)
        await loader.waitForApplies(2)

        #expect(loader.appliedUpdates.last == update)
        #expect(first.serveCount == 0)
        #expect(second.serveCount == 0)
    }

    @Test func updateOfAnotherExecutableOfAnOpenProductIsApplied() async {
        _ = coordinator.register(productId: Self.productId, contentId: Self.appId, reloader: first)
        coordinator.start()

        let workerUpdate = Self.makeUpdate(contentId: Self.workerId)
        loader.simulate(update: workerUpdate)
        await loader.waitForApplies(1)

        #expect(loader.appliedUpdates == [workerUpdate])
        #expect(!coordinator.applyPendingUpdate(of: Self.appId))
    }

    @Test func refreshAppliesAPendingUpdateAndReloadsEveryOpenSPA() async {
        _ = coordinator.register(productId: Self.productId, contentId: Self.appId, reloader: first)
        _ = coordinator.register(productId: Self.productId, contentId: Self.appId, reloader: second)
        coordinator.start()

        await deliver(Self.makeUpdate())

        #expect(coordinator.applyPendingUpdate(of: Self.appId))
        await first.waitForServes(1)
        await second.waitForServes(1)

        #expect(loader.appliedUpdates.count == 2)
        #expect(!coordinator.applyPendingUpdate(of: Self.appId))
    }

    @Test func returningToTheForegroundRefreshesOpenProducts() async {
        _ = coordinator.register(productId: Self.productId, contentId: Self.appId, reloader: first)
        coordinator.start()

        NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        await loader.waitForRefreshes(1)

        #expect(loader.refreshedIds == [Self.productId])
    }
}

private extension ProductUpdateCoordinatorTests {
    /// Updates are handled in arrival order, so applying the next one means `update` was handled.
    func deliver(_ update: ExecutableRelease) async {
        let applied = loader.appliedUpdates.count

        loader.simulate(update: update)
        loader.simulate(update: Self.makeUpdate(contentId: Self.otherAppId))
        await loader.waitForApplies(applied + 1)
    }

    static func makeCoordinator(loader: StubProductContentLoader) -> ProductUpdateCoordinator {
        ProductUpdateCoordinator(
            contentLoader: loader,
            applicationStateStreams: ApplicationStateStreamFactory(),
            logger: Logger.shared
        )
    }

    static func makeUpdate(contentId: ProductId = appId) -> ExecutableRelease {
        ExecutableRelease(
            contentId: contentId,
            archive: DotNsResolvedContent(contentHash: "02", localURL: URL(fileURLWithPath: "/content/02"))
        )
    }
}
