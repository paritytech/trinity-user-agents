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
    private static let hostPlacedId = "dim2.dot"

    private let loader = StubProductContentLoader()
    private let workerReloader = StubProductWorkerReloader()
    private let notice = StubProductUpdateNoticePresenter()
    private let hostPlaced = StubHostPlacedProducts()
    private let coordinator: ProductUpdateCoordinator
    private let first = StubSPAProductReloader()
    private let second = StubSPAProductReloader()

    init() {
        coordinator = ProductUpdateCoordinator(
            contentLoader: loader,
            workerReloader: workerReloader,
            noticePresenter: notice,
            refreshSources: ProductUpdateCoordinator.RefreshSources(
                applicationStateStreams: ApplicationStateStreamFactory(),
                hostPlacedProducts: hostPlaced
            ),
            logger: Logger.shared
        )
    }

    @Test func workerUpdateIsAppliedAndReloadsTheWorker() async {
        let update = Self.makeUpdate(contentId: Self.workerId)
        coordinator.start()

        loader.simulate(update: update)
        await workerReloader.waitForReloads(1)

        #expect(loader.appliedUpdates == [update])
        #expect(workerReloader.reloadedIds == [Self.productId])
        #expect(notice.shownProductNames.isEmpty)
    }

    @Test func appUpdateWithNoOpenSPAIsAppliedWithoutReloadingTheWorker() async {
        let update = Self.makeUpdate()
        coordinator.start()

        await deliver(update)

        #expect(loader.appliedUpdates.first == update)
        #expect(workerReloader.reloadedIds.isEmpty)
    }

    @Test func legacyBaseNameUpdateReloadsTheWorker() async {
        coordinator.start()

        loader.simulate(update: Self.makeUpdate(contentId: Self.productId))
        await workerReloader.waitForReloads(1)

        #expect(workerReloader.reloadedIds == [Self.productId])
    }

    @Test func updateForAnOpenProductShowsTheNoticeInsteadOfApplying() async {
        register(first)
        coordinator.start()

        loader.simulate(update: Self.makeUpdate())
        await notice.waitForShows(1)

        #expect(notice.shownProductNames == ["Shop"])
        #expect(notice.shownProductIds == [Self.productId])
        #expect(loader.appliedUpdates.isEmpty)
        #expect(first.serveCount == 0)
    }

    @Test func tappingUpdateAppliesAndReloadsEveryOpenSPA() async {
        register(first)
        register(second)
        coordinator.start()

        let update = Self.makeUpdate()
        loader.simulate(update: update)
        await notice.waitForShows(1)

        notice.simulateUpdate()
        await loader.waitForApplies(1)
        await first.waitForServes(1)
        await second.waitForServes(1)

        #expect(loader.appliedUpdates == [update])
        #expect(!notice.isShowing)
    }

    @Test func dismissedUpdateIsAppliedOnceTheLastSPACloses() async {
        let firstId = register(first)
        let secondId = register(second)
        coordinator.start()

        let update = Self.makeUpdate()
        loader.simulate(update: update)
        await notice.waitForShows(1)

        notice.simulateDismiss()
        coordinator.unregister(firstId)
        await deliver(Self.makeUpdate(contentId: Self.otherAppId))

        #expect(!notice.isShowing)
        #expect(!loader.appliedUpdates.contains(update))

        coordinator.unregister(secondId)
        // Both releases `deliver` sent are applied too, in either order.
        await loader.waitForApplies(3)

        #expect(loader.appliedUpdates.contains(update))
        #expect(first.serveCount == 0)
        #expect(second.serveCount == 0)
    }

    @Test func updateOfAnotherExecutableOfAnOpenProductIsApplied() async {
        register(first)
        coordinator.start()

        let update = Self.makeUpdate(contentId: Self.workerId)
        loader.simulate(update: update)
        await workerReloader.waitForReloads(1)

        #expect(loader.appliedUpdates == [update])
        #expect(notice.shownProductNames.isEmpty)
    }

    @Test func refreshWithAPendingUpdateAppliesIt() async {
        register(first)
        coordinator.start()

        loader.simulate(update: Self.makeUpdate())
        await notice.waitForShows(1)

        #expect(coordinator.applyPendingUpdate(of: Self.appId))
        await first.waitForServes(1)

        #expect(!coordinator.applyPendingUpdate(of: Self.appId))
    }

    @Test func supersededContentIsRemovedKeepingWhatOpenSPAsServeAndPendingUpdates() async {
        first.servedContentHash = "01"
        register(first)
        coordinator.start()

        loader.simulate(update: Self.makeUpdate())
        await notice.waitForShows(1)
        loader.simulate(update: Self.makeUpdate(contentId: Self.workerId))
        await loader.waitForRemovals(2)

        #expect(loader.keptHashesOnRemoval == [["01"], ["01", "02"]])
    }

    @Test func returningToTheForegroundRefreshesOpenProducts() async {
        register(first)
        coordinator.start()

        NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        await loader.waitForRefreshes(1)

        #expect(loader.refreshedIds == [Self.productId])
    }

    @Test func returningToTheForegroundRefreshesHostPlacedProductsWithNothingOpen() async {
        hostPlaced.productIds = [Self.hostPlacedId]
        coordinator.start()

        NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        await loader.waitForRefreshes(1)

        #expect(loader.refreshedIds == [Self.hostPlacedId])
    }
}

private extension ProductUpdateCoordinatorTests {
    @discardableResult
    func register(_ reloader: StubSPAProductReloader) -> UUID {
        coordinator.register(productId: Self.productId, contentId: Self.appId, displayName: "Shop", reloader: reloader)
    }

    /// Updates are handled in arrival order, so applying the next one means `update` was handled.
    func deliver(_ update: ExecutableRelease) async {
        let applied = loader.appliedUpdates.count

        loader.simulate(update: update)
        loader.simulate(update: Self.makeUpdate(contentId: Self.otherAppId))
        await loader.waitForApplies(applied + 1)
    }

    static func makeUpdate(contentId: ProductId = appId) -> ExecutableRelease {
        ExecutableRelease(
            contentId: contentId,
            archive: DotNsResolvedContent(contentHash: "02", localURL: URL(fileURLWithPath: "/content/02"))
        )
    }
}

private final class StubHostPlacedProducts: HostPlacedProductsProviding, @unchecked Sendable {
    var productIds: [ProductId] = []

    func placedProductIds() async -> [ProductId] {
        productIds
    }
}
