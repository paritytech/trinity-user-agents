import AsyncExtensions
import ChainRegistry
import Foundation
import os
import PolkadotUI
import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import UIKit
@testable import polkadot_app

/// The Wallet tab is the Pocket's only surface, and it follows the collection
/// from storage. A card entering or leaving reaches it whoever put it there,
/// including the core removing one through a product's own bridge.
@MainActor
struct WalletMainPocketTests {
    /// Warming fetches an archive per host-placed product. A change that landed
    /// while the tab was still busy warming the last one must still reach it,
    /// or the tab sits on a collection that has already moved.
    @Test
    func showsAChangeThatLandedWhileItWasStillWarmingTheLastOne() async throws {
        let collection = InMemoryPocketCardStore()
        let presenter = RecordingPresenter()
        let interactor = makeInteractor(
            collection: collection,
            presenter: presenter,
            warmDelay: .milliseconds(400)
        )

        interactor.setup()
        try await presenter.waitForCards { _ in true }
        let beforeTheChange = presenter.pocketCards.count

        try await collection.add(loyalty, face: .nil)
        try await presenter.waitForCards { $0.contains(loyalty.key) }

        #expect(presenter.pocketCards.count > beforeTheChange)
        #expect(presenter.pocketCards.last?.contains { $0.key == loyalty.key } == true)
    }

    /// A removal the core made through a product's bridge writes to the same
    /// storage and nothing announces it, so the tab has to see it by following.
    @Test
    func showsACardLeavingTheCollection() async throws {
        let collection = InMemoryPocketCardStore([loyalty])
        let presenter = RecordingPresenter()
        let interactor = makeInteractor(collection: collection, presenter: presenter)

        interactor.setup()
        try await presenter.waitForCards { $0.contains(loyalty.key) }
        #expect(presenter.pocketCards.last?.contains { $0.key == loyalty.key } == true)

        _ = try await collection.removeCard(loyalty.key)
        try await presenter.waitForCards { !$0.contains(loyalty.key) }

        #expect(presenter.pocketCards.last?.contains { $0.key == loyalty.key } == false)
    }

    /// The collectibles page is the stash product, named under each network's
    /// own suffix, so a fixed suffix opens a page that exists on one network
    /// only. It reads the collection over the host's chain connection, which it
    /// gets only when opened as a product.
    @Test(arguments: ["paseo", "testnet"])
    func viewingCollectiblesOpensStashOnTheActiveNetworkAsAProduct(network: String) async throws {
        let screen = CollectiblesScreen(tld: StubDotNsTldProvider(tld: network))

        screen.presenter.setup()
        try await screen.view.waitForCollectibles()
        screen.presenter.showCollectibles()

        #expect(screen.view.showsCollectibles)
        #expect(screen.wireframe.openedProduct == "stash.\(network)")
    }

    /// The remote switch pulls the card from every installed app without a
    /// release. A switched-off app must not read the chain either, since the
    /// suffix read is retried for as long as it goes unanswered.
    @Test
    func theRemoteSwitchPullsTheCollectiblesCardWithoutReadingTheChain() async throws {
        let tld = LateTld()
        let screen = CollectiblesScreen(tld: tld, collectiblesEnabled: false)

        screen.presenter.setup()

        #expect(!screen.view.showsCollectibles)
        #expect(tld.reads == 0)
    }

    /// The tab asks once, often before the chain has answered, and giving up
    /// then would hide the card until the app restarts.
    @Test
    func showsTheCollectiblesCardOnceALateNetworkSuffixArrives() async throws {
        let screen = CollectiblesScreen(tld: LateTld())

        screen.presenter.setup()
        try await screen.view.waitForCollectibles()

        #expect(screen.view.showsCollectibles)
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardEntry(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false
)

@MainActor
private func makeInteractor(
    collection: InMemoryPocketCardStore = InMemoryPocketCardStore(),
    presenter: RecordingPresenter? = nil,
    warmDelay: Duration = .zero,
    tld: DotNsTldProviding = StubDotNsTldProvider(tld: nil),
    collectiblesEnabled: Bool = false
) -> WalletMainInteractor {
    let remoteConfig = MockRemoteConfigManager()
    remoteConfig.collectiblesEnabled = collectiblesEnabled

    let interactor = WalletMainInteractor(
        networkStatusObserver: StubNetworkStatusObserver(),
        pocketPrewarmer: PocketPrewarmer(
            products: StubProductResolver(),
            dotNsResolver: SlowArchives(delay: warmDelay)
        ),
        pocket: ProductPocketService(collection: collection),
        tldProvider: tld,
        hostProvider: ProductHostFactory(tldProvider: tld),
        remoteConfig: remoteConfig
    )
    interactor.presenter = presenter
    return interactor
}

@MainActor
private final class RecordingPresenter: WalletMainInteractorOutputProtocol {
    private(set) var pocketCards: [[PocketCardViewModel]] = []
    private let shown: AsyncStream<Set<PocketCardKey>>
    private let continuation: AsyncStream<Set<PocketCardKey>>.Continuation

    init() {
        (shown, continuation) = AsyncStream<Set<PocketCardKey>>.makeStream()
    }

    func didReceive(networkStatus _: NetworkStatus) {}
    func didReceiveCollectibles(page _: ProductPage) {}

    func didReceive(pocketCards: [PocketCardViewModel]) {
        self.pocketCards.append(pocketCards)
        continuation.yield(Set(pocketCards.map(\.key)))
    }

    /// Returns once the presenter has shown a card list matching `predicate`,
    /// failing rather than hanging if it never does.
    func waitForCards(where predicate: @escaping @Sendable (Set<PocketCardKey>) -> Bool) async throws {
        try await withTimeout(.hangGuard) { [shown] in
            for await keys in shown where predicate(keys) {
                return
            }
            throw CancellationError()
        }
    }
}

/// The Wallet tab from its presenter down, with the screen and the navigation
/// it would reach recorded.
@MainActor
private struct CollectiblesScreen {
    let view = RecordingWalletView()
    let wireframe = RecordingWireframe()
    let presenter: WalletMainPresenter

    init(tld: DotNsTldProviding, collectiblesEnabled: Bool = true) {
        let interactor = makeInteractor(tld: tld, collectiblesEnabled: collectiblesEnabled)
        presenter = WalletMainPresenter(
            interactor: interactor,
            wireframe: wireframe,
            titleViewModelFactory: NetworkStatusTitleViewModelFactory(screenTitle: "")
        )
        interactor.presenter = presenter
        presenter.view = view
    }
}

private final class RecordingWalletView: UIViewController, WalletMainViewProtocol {
    private(set) var showsCollectibles = false
    private let availability: AsyncStream<Bool>
    private let continuation: AsyncStream<Bool>.Continuation

    init() {
        (availability, continuation) = AsyncStream<Bool>.makeStream()
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    func didReceive(isCollectiblesAvailable: Bool) {
        showsCollectibles = isCollectiblesAvailable
        continuation.yield(isCollectiblesAvailable)
    }

    /// Returns once the collectibles card has been offered, failing rather than hanging if it never is.
    func waitForCollectibles(within limit: Duration = .hangGuard) async throws {
        try await withTimeout(limit) { [availability] in
            for await isAvailable in availability where isAvailable {
                return
            }
            throw CancellationError()
        }
    }

    func didReceive(pocketCards _: [PocketCardViewModel]) {}
    func didReceive(titleViewModel _: NetworkStatusTitleView.ViewModel) {}
    func frame(of _: PocketCardKey) -> CGRect? { nil }
    func setOpenCard(_: PocketCardKey?) {}
    func moveOpenCard(by _: CGSize, clearingPocket _: Bool, animated _: Bool) {}
}

@MainActor
private final class RecordingWireframe: WalletMainWireframeProtocol {
    private(set) var openedProduct: String?

    func showProduct(page: ProductPage) {
        openedProduct = page.host.toDotDomain()
    }

    func showPocketCard(_: PocketCardViewModel, from _: WalletMainViewProtocol?) {}
    func confirmPocketCardRemoval(_: PocketCardViewModel, onConfirm _: @escaping () -> Void) {}
}

/// A chain that has not answered the tab's first read, then does.
private final class LateTld: DotNsTldProviding {
    private let readCount = OSAllocatedUnfairLock(initialState: 0)

    var reads: Int {
        readCount.withLock { $0 }
    }

    func currentTld() -> String? {
        readCount.withLock { count in
            defer { count += 1 }
            return count == 0 ? nil : "paseo"
        }
    }

    func resolveTld() async throws -> String {
        readCount.withLock { $0 += 1 }
        throw DotNsTldError.unavailable
    }

    func refresh() {}
    func reset() {}
}

private final class StubNetworkStatusObserver: NetworkStatusObserving {
    func start(onStatus _: @escaping @MainActor (NetworkStatus) -> Void) {}
}

/// Stands in for the archive fetch a warm performs, which is the slow part the
/// collection must not be read behind.
private final class SlowArchives: DotNsResolverProtocol, @unchecked Sendable {
    let delay: Duration

    init(delay: Duration) {
        self.delay = delay
    }

    func resolveToLocalURL(dotNsName: String) async throws -> URL {
        if delay > .zero { try await Task.sleep(for: delay) }

        return URL(fileURLWithPath: "/tmp/\(dotNsName)")
    }

    func getMetadataEntry(dotNsName _: String, key _: String) async throws -> String? { nil }
    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func clearCache() throws {}
}
