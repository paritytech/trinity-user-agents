import Foundation

final class WalletMainInteractor {
    weak var presenter: WalletMainInteractorOutputProtocol?

    private let collectiblesURLProvider: CollectiblesURLProviding
    private let networkStatusObserver: NetworkStatusObserving
    private let pocketPrewarmer: PocketPrewarmer
    private let pocket: ProductPocketService?
    private var resolutionTask: Task<Void, Never>?
    private var pocketTask: Task<Void, Never>?
    private var warming: Task<Void, Never>?

    init(
        collectiblesURLProvider: CollectiblesURLProviding,
        networkStatusObserver: NetworkStatusObserving,
        pocketPrewarmer: PocketPrewarmer,
        pocket: ProductPocketService?
    ) {
        self.collectiblesURLProvider = collectiblesURLProvider
        self.networkStatusObserver = networkStatusObserver
        self.pocketPrewarmer = pocketPrewarmer
        self.pocket = pocket
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

        #if FEATURE_DIMS
            resolutionTask = Task { [weak self, collectiblesURLProvider] in
                let url = await collectiblesURLProvider.resolveURL()

                guard !Task.isCancelled else { return }

                await self?.presenter?.didReceiveCollectibles(url: url)
            }
        #endif

        followPocket()
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
