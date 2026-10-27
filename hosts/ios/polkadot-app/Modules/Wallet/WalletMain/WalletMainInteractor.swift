import Foundation
import Products

final class WalletMainInteractor {
    weak var presenter: WalletMainInteractorOutputProtocol?

    private let networkStatusObserver: NetworkStatusObserving
    private let pocketPrewarmer: PocketPrewarmer
    private let pocket: ProductPocketService?
    private let tldProvider: DotNsTldProviding
    private let hostProvider: ProductHostProviding
    private let remoteConfig: RemoteConfigManaging
    private var resolutionTask: Task<Void, Never>?
    private var pocketTask: Task<Void, Never>?
    private var warming: Task<Void, Never>?

    init(
        networkStatusObserver: NetworkStatusObserving,
        pocketPrewarmer: PocketPrewarmer,
        pocket: ProductPocketService?,
        tldProvider: DotNsTldProviding,
        hostProvider: ProductHostProviding,
        remoteConfig: RemoteConfigManaging
    ) {
        self.networkStatusObserver = networkStatusObserver
        self.pocketPrewarmer = pocketPrewarmer
        self.pocket = pocket
        self.tldProvider = tldProvider
        self.hostProvider = hostProvider
        self.remoteConfig = remoteConfig
    }

    deinit {
        resolutionTask?.cancel()
        pocketTask?.cancel()
        warming?.cancel()
    }
}

extension WalletMainInteractor: WalletMainInteractorInputProtocol {
    func setup() {
        guard resolutionTask == nil else { return }

        networkStatusObserver.start { [weak self] status in
            self?.presenter?.didReceive(networkStatus: status)
        }

        followPocket()

        showCollectibles()
    }

    /// Removal is the store's, not the tab's: the collection is followed, so
    /// the card leaves the screen when it leaves storage rather than because
    /// this said so.
    func removePocketCard(_ card: PocketCardViewModel) {
        guard let collection = pocket?.collection else { return }

        Task { _ = try? await collection.removeCard(card.key) }
    }
}

private extension WalletMainInteractor {
    /// The collectibles page is the stash product on the active network. The
    /// tab asks once, so the network is waited for rather than given up on.
    func showCollectibles() {
        guard remoteConfig.syncedCollectiblesEnabled() else { return }

        resolutionTask = Task { [weak presenter, tldProvider, hostProvider] in
            guard
                await tldProvider.tldRetrying() != nil,
                let host = hostProvider.host(label: AppConfig.DotNs.dotNsCollectibles),
                let presenter
            else { return }

            await MainActor.run {
                presenter.didReceiveCollectibles(page: ProductPage(host: host))
            }
        }
    }

    /// The cards come from the Pocket rather than from storage directly, so the
    /// tab shows what the host holds without waiting on any product's worker,
    /// and shows every change whoever made it, the core removing a card
    /// included.
    func followPocket() {
        guard let pocket else { return }

        pocketTask = Task { [weak self] in
            do {
                for try await cards in pocket.cards() {
                    guard let self else { return }

                    await show(cards)
                }
            } catch {
                Logger.shared.error("[pocket] the wallet tab stopped following the collection: \(error)")
            }
        }
    }

    func show(_ cards: [PocketCardViewModel]) async {
        let held = Set(cards.map(\.key))

        await MainActor.run { [pocket] in
            presenter?.didReceive(pocketCards: cards)
            pocket?.cardHosts.keepOnly { held.contains($0) }
        }

        prewarm(cards)
    }

    /// Warming fetches an archive, so it runs beside the collection rather than
    /// in front of the next change: held here, every change that landed while
    /// it ran would arrive late, and the tab would sit on a stale collection.
    func prewarm(_ cards: [PocketCardViewModel]) {
        warming?.cancel()
        warming = Task { [pocketPrewarmer] in
            await pocketPrewarmer.warm(cards)
        }
    }
}
