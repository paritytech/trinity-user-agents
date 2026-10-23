import Foundation
import FoundationExt
import Products
import Testing
import UIKit
@testable import polkadot_app

@MainActor
struct ProductUpdateCoordinatorTests {
    private static let productId = "shop.dot"

    private let loader: StubProductContentLoader
    private let workerReloader = StubProductWorkerReloader()
    private let notice = StubProductUpdateNoticePresenter()
    private let coordinator: ProductUpdateCoordinator

    init() {
        self.init(pendingUpdates: [])
    }

    private init(pendingUpdates: [ProductUpdate]) {
        loader = StubProductContentLoader(pendingUpdates: pendingUpdates)
        coordinator = ProductUpdateCoordinator(
            contentLoader: loader,
            workerReloader: workerReloader,
            noticePresenter: notice,
            applicationStateStreams: ApplicationStateStreamFactory(),
            logger: Logger.shared
        )
    }

    @Test func updateForAProductWithNoOpenSPAIsAppliedAndReloadsTheWorker() async {
        let update = Self.makeUpdate()
        coordinator.start()

        loader.simulate(update: update)
        await workerReloader.waitForReloads(1)

        #expect(loader.appliedUpdates == [update])
        #expect(workerReloader.reloadedIds == [Self.productId])
        #expect(notice.shownProductNames.isEmpty)
    }

    @Test func updateForAnOpenProductShowsTheNoticeInsteadOfApplying() async {
        let reloader = StubSPAProductReloader()
        _ = coordinator.register(productId: Self.productId, reloader: reloader)
        coordinator.start()

        loader.simulate(update: Self.makeUpdate())
        await notice.waitForShows(1)

        #expect(notice.shownProductNames == ["Shop"])
        #expect(loader.appliedUpdates.isEmpty)
        #expect(reloader.serveCount == 0)
    }

    @Test func tappingUpdateAppliesAndReloadsEveryOpenSPAAndTheWorker() async {
        let first = StubSPAProductReloader()
        let second = StubSPAProductReloader()
        _ = coordinator.register(productId: Self.productId, reloader: first)
        _ = coordinator.register(productId: Self.productId, reloader: second)
        coordinator.start()

        let update = Self.makeUpdate()
        loader.simulate(update: update)
        await notice.waitForShows(1)

        notice.simulateUpdate()
        await workerReloader.waitForReloads(1)

        #expect(loader.appliedUpdates == [update])
        #expect(first.serveCount == 1)
        #expect(second.serveCount == 1)
        #expect(!notice.isShowing)
        #expect(notice.shownProductNames.count == 1)
    }

    @Test func dismissedUpdateIsAppliedOnceTheLastSPACloses() async {
        let first = StubSPAProductReloader()
        let second = StubSPAProductReloader()
        let firstId = coordinator.register(productId: Self.productId, reloader: first)
        let secondId = coordinator.register(productId: Self.productId, reloader: second)
        coordinator.start()

        let update = Self.makeUpdate()
        loader.simulate(update: update)
        await notice.waitForShows(1)

        notice.simulateDismiss()
        coordinator.unregister(firstId)

        #expect(!notice.isShowing)
        #expect(loader.appliedUpdates.isEmpty)

        coordinator.unregister(secondId)
        await workerReloader.waitForReloads(1)

        #expect(loader.appliedUpdates == [update])
        #expect(first.serveCount == 0)
        #expect(second.serveCount == 0)
    }

    @Test func refreshWithAPendingUpdateAppliesItInsteadOfReloadingAlone() async {
        let reloader = StubSPAProductReloader()
        _ = coordinator.register(productId: Self.productId, reloader: reloader)
        coordinator.start()

        loader.simulate(update: Self.makeUpdate())
        await notice.waitForShows(1)

        #expect(coordinator.applyPendingUpdate(of: Self.productId))
        await workerReloader.waitForReloads(1)

        #expect(reloader.serveCount == 1)
        #expect(!coordinator.applyPendingUpdate(of: Self.productId))
    }

    @Test func supersededContentIsRemovedAtStartAndAfterAnApplyKeepingWhatOpenSPAsServe() async {
        let reloader = StubSPAProductReloader()
        reloader.servedContentHash = "01"
        _ = coordinator.register(productId: "other.dot", reloader: reloader)
        coordinator.start()

        loader.simulate(update: Self.makeUpdate())
        await workerReloader.waitForReloads(1)
        await loader.waitForRemovals(2)

        #expect(loader.keptHashesOnRemoval == [["01"], ["01"]])
    }

    @Test func updatePublishedBeforeStartIsStillHandled() async {
        let update = Self.makeUpdate()
        let coordinator = ProductUpdateCoordinator(
            contentLoader: StubProductContentLoader(pendingUpdates: [update]),
            workerReloader: workerReloader,
            noticePresenter: notice,
            applicationStateStreams: ApplicationStateStreamFactory(),
            logger: Logger.shared
        )

        coordinator.start()
        await workerReloader.waitForReloads(1)

        #expect(workerReloader.reloadedIds == [Self.productId])
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
    static func makeUpdate() -> ProductUpdate {
        let resolved = ResolvedProduct.legacy(id: productId, displayName: "Shop")
        let previous = ProductVersion(productId: productId, records: [:], contentHashes: [productId: "01"])
        let latest = ProductVersion(productId: productId, records: [:], contentHashes: [productId: "02"])

        return ProductUpdate(
            previous: previous,
            latest: FetchedProductVersion(
                version: latest,
                resolved: resolved,
                appContent: DotNsResolvedContent(contentHash: "02", localURL: URL(fileURLWithPath: "/content/02"))
            )
        )
    }
}
