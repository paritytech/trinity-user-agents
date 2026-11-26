import struct TrUAPIHost.ProductId
import Foundation
import FoundationExt
import Products
import Testing
import UIKit
@testable import polkadot_app

@MainActor
final class ProductUpdateCoordinatorTests {
    private static let productId = try! ProductId("shop.dot")
    private nonisolated static let appId = try! ProductId("app.shop.dot")
    private static let workerId = try! ProductId("worker.shop.dot")
    private static let otherAppId = try! ProductId("app.other.dot")
    private static let hostPlacedId = try! ProductId("dim2.dot")

    private let loader = StubProductContentLoader()
    private let workerReloader = StubProductWorkerReloader()
    private let notice = StubProductUpdateNoticePresenter()
    private let hostPlaced = StubHostPlacedProducts()
    private let notificationCenter = NotificationCenter()
    private lazy var coordinator = makeCoordinator(loader: loader) { [workerReloader] in workerReloader }
    private let first = StubSPAProductReloader()
    private let second = StubSPAProductReloader()

    @Test(arguments: [true, false])
    func workerUpdateUsesTheSessionActiveWhenApplyFinishes(replacementIsSignedIn: Bool) async {
        let gate = AsyncStream<Void>.makeStream()
        let loader = StubProductContentLoader(applyGate: gate.stream)
        let replacement = StubProductWorkerReloader()
        var active: StubProductWorkerReloader? = workerReloader
        let coordinator = makeCoordinator(loader: loader) { active }
        _ = coordinator.register(productId: Self.productId, contentId: Self.appId, displayName: "Shop", reloader: first)
        let update = Self.makeUpdate(contentId: Self.workerId)
        coordinator.start()

        loader.simulate(update: update)
        await loader.waitForApplies(1)
        active = replacementIsSignedIn ? replacement : nil
        gate.continuation.finish()
        await loader.waitForRemovals(2)

        #expect(loader.appliedUpdates == [update])
        #expect(notice.shownProductNames.isEmpty)
        #expect(workerReloader.reloadedIds.isEmpty)
        #expect(replacement.reloadedIds == (replacementIsSignedIn ? [Self.productId] : []))
    }

    @Test func appUpdateWithNoOpenSPAIsAppliedWithoutReloadingTheWorker() async {
        let update = Self.makeUpdate()
        coordinator.start()

        await deliver(update)

        #expect(loader.appliedUpdates == [update])
        #expect(workerReloader.reloadedIds.isEmpty)
    }

    @Test func legacyBaseNameUpdateReloadsTheWorker() async {
        coordinator.start()

        await deliver(Self.makeUpdate(contentId: Self.productId))

        #expect(workerReloader.reloadedIds == [Self.productId])
    }

    @Test func tappingUpdateAppliesAndReloadsEveryOpenSPA() async {
        register(first)
        register(second)
        coordinator.start()

        let update = Self.makeUpdate()
        loader.simulate(update: update)
        await notice.waitForShows(1)

        #expect(notice.shownProductNames == ["Shop"])
        #expect(notice.shownProductIds == [Self.productId])
        #expect(loader.appliedUpdates.isEmpty)
        #expect(first.serveCount == 0)
        #expect(second.serveCount == 0)

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
        await loader.waitForApplies(2)

        #expect(loader.appliedUpdates.contains(update))
        #expect(first.serveCount == 0)
        #expect(second.serveCount == 0)
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

        notificationCenter.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        await loader.waitForRefreshes(1)

        #expect(loader.refreshedIds == [Self.productId])
    }

    @Test func returningToTheForegroundRefreshesHostPlacedProductsWithNothingOpen() async {
        hostPlaced.productIds = [Self.hostPlacedId]
        coordinator.start()

        notificationCenter.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        await loader.waitForRefreshes(1)

        #expect(loader.refreshedIds == [Self.hostPlacedId])
    }
}

private extension ProductUpdateCoordinatorTests {
    func makeCoordinator(
        loader: StubProductContentLoader,
        currentWorkerReloader: @escaping () -> ProductWorkerReloading?
    ) -> ProductUpdateCoordinator {
        ProductUpdateCoordinator(
            contentLoader: loader,
            currentWorkerReloader: currentWorkerReloader,
            noticePresenter: notice,
            refreshSources: .init(
                applicationStateStreams: ApplicationStateStreamFactory(notificationCenter: notificationCenter),
                hostPlacedProducts: hostPlaced
            ),
            logger: Logger.shared
        )
    }

    @discardableResult
    func register(_ reloader: StubSPAProductReloader) -> UUID {
        coordinator.register(productId: Self.productId, contentId: Self.appId, displayName: "Shop", reloader: reloader)
    }

    func deliver(_ update: ExecutableRelease) async {
        await loader.waitForRemovals(1)
        let removals = loader.keptHashesOnRemoval.count
        loader.simulate(update: update)
        await loader.waitForRemovals(removals + 1)
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
