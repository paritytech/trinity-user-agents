import Foundation
import PolkadotUI
import Products

/// One card as the Wallet tab draws it.
struct PocketCardViewModel: Identifiable {
    let key: PocketCardKey
    let title: String
    let privileged: Bool
    let face: CustomMessageWidgetNode?

    var id: String { key.storageId }
}

/// Assembles the collection the Wallet tab shows: the cards the host holds,
/// each with the newest face it has for them.
///
/// The face is whatever is held right now, bundled for a pinned card, approved
/// for an added one, or the last one its product drew, so the tab draws
/// immediately, offline and at cold start, without waiting on any worker.
struct PocketCardsProvider {
    private let store: any PocketCardStore
    private let resolver: any WidgetDesignTokenResolving
    private let logger: LoggerProtocol

    init(
        store: any PocketCardStore,
        resolver: any WidgetDesignTokenResolving = WidgetDesignTokenResolver(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.store = store
        self.resolver = resolver
        self.logger = logger
    }

    /// A collection that cannot be read draws as none. The tab has nowhere to
    /// put a storage failure, and an empty Pocket for one launch is better than
    /// a tab that does not come up.
    func cards() async -> [PocketCardViewModel] {
        await cards(held())
    }

    /// The collection as it was handed over, which is what a caller following
    /// it already has.
    func cards(_ held: [PocketCardEntry]) async -> [PocketCardViewModel] {
        var drawn: [PocketCardViewModel] = []
        for card in held {
            await drawn.append(viewModel(for: card))
        }
        return drawn
    }

    /// The one card `key` names, for a link that arrives naming a single card
    /// rather than the whole collection.
    func card(for key: PocketCardKey) async -> PocketCardViewModel? {
        guard let card = await held().first(where: { $0.key == key }) else { return nil }

        return await viewModel(for: card)
    }

    private func held() async -> [PocketCardEntry] {
        do {
            return try await store.cards()
        } catch {
            logger.error("[pocket] the collection could not be read; the tab draws none: \(error)")
            return []
        }
    }

    private func viewModel(for card: PocketCardEntry) async -> PocketCardViewModel {
        let face = await store.face(for: card.key)

        return PocketCardViewModel(
            key: card.key,
            title: card.title,
            privileged: card.privileged,
            face: face?.toWidgetNode(resolver: resolver)
        )
    }
}
