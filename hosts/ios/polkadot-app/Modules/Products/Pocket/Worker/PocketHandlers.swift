import Foundation
import AsyncExtensions
import Products
import TrUAPIHost

/// Owns Pocket's worker claims for the sides of collected cards currently on screen.
@MainActor
final class PocketHandlers: PocketFaceStreamsResolving {
    private struct Presentation {
        let key: PocketCardKey
        let expanded: Bool
    }

    private final class LiveCard {
        var lookup: Task<Void, Never>?
        var front: (any TrUAPIWorkerClaiming)?
        var back: (any TrUAPIWorkerClaiming)?
        var hasBack: Bool?
        var refused = false

        func close() {
            lookup?.cancel()
            front?.release()
            back?.release()
        }
    }

    private let workers: any TrUAPIWorkerManaging
    private let published: any PublishedPocketCardsResolving
    private let logger: LoggerProtocol
    private nonisolated let handler: TrUAPIPocketHandler
    private nonisolated let announced = AsyncCurrentValueSubject<[PocketCardKey: Bool]>([:])
    private var collected: Set<PocketCardKey> = []
    private var presentations: [UUID: Presentation] = [:]
    private var live: [PocketCardKey: LiveCard] = [:]
    private var stopped = false

    nonisolated init(
        workers: any TrUAPIWorkerManaging,
        published: any PublishedPocketCardsResolving,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.workers = workers
        self.published = published
        self.logger = logger
        handler = TrUAPIPocketHandler(workers: workers, logger: logger)
    }

    nonisolated func streams(for key: PocketCardKey) -> (any PocketFaceStreaming)? {
        announced.value[key] == true ? handler : nil
    }

    nonisolated func awaitStreams(for key: PocketCardKey) async -> (any PocketFaceStreaming)? {
        for await cards in announced {
            if let offered = cards[key] { return offered ? handler : nil }
        }
        return nil
    }

    func present(_ key: PocketCardKey, expanded: Bool) -> @MainActor () -> Void {
        guard !stopped else { return {} }
        let id = UUID()
        presentations[id] = Presentation(key: key, expanded: expanded)
        update(key)
        return { [weak self] in
            guard let self, presentations.removeValue(forKey: id) != nil else { return }

            update(key)
        }
    }

    func settle(_ cards: [PocketCardEntry]) {
        let previous = collected
        collected = Set(cards.map(\.key))
        for key in previous.symmetricDifference(collected) {
            update(key)
        }
        for key in previous.intersection(collected) {
            guard let card = live[key], card.lookup == nil else { continue }
            activate(card, for: key, refreshOffer: true)
        }
    }

    func stop() {
        stopped = true
        for card in live.values {
            card.close()
        }
        live.removeAll()
        presentations.removeAll()
        announced.send([:])
        announced.send(.finished)
    }

    private func update(_ key: PocketCardKey) {
        guard !stopped else { return }
        let views = presentations.values.filter { $0.key == key }
        guard collected.contains(key), !views.isEmpty else {
            if let card = live.removeValue(forKey: key) {
                card.close()
                announce()
            }
            return
        }

        let card = live[key] ?? LiveCard()
        live[key] = card
        let expanded = views.contains(where: \.expanded)
        if !expanded {
            card.back?.release()
            card.back = nil
        }
        guard !card.refused, card.lookup == nil,
              card.front == nil || (expanded && card.hasBack == true && card.back == nil) else { return }
        activate(card, for: key, refreshOffer: false)
    }

    private func activate(_ card: LiveCard, for key: PocketCardKey, refreshOffer: Bool) {
        card.lookup = Task { [weak self] in
            await self?.claimSides(of: card, for: key, refreshOffer: refreshOffer)
        }
    }

    private func claimSides(of card: LiveCard, for key: PocketCardKey, refreshOffer: Bool) async {
        defer { card.lookup = nil }
        var needsOffer = refreshOffer || card.hasBack == nil
        var attempt = 0
        while !Task.isCancelled {
            do {
                if needsOffer {
                    let offered = try await published.find(productId: key.productId, cardId: key.cardId)
                    needsOffer = false
                    card.refused = false
                    card.hasBack = offered.definition.back != nil
                }
                guard live[key] === card, !Task.isCancelled else { return }
                if card.front == nil {
                    card.front = try workers.claim(key.productId)
                    announce()
                }
                if card.hasBack == true, presentations.values.contains(where: { $0.key == key && $0.expanded }),
                   card.back == nil {
                    card.back = try workers.claim(key.productId)
                } else if card.hasBack == false {
                    card.back?.release()
                    card.back = nil
                }
                return
            } catch is PocketPublishError {
                guard live[key] === card else { return }
                card.refused = true
                card.front?.release()
                card.front = nil
                card.back?.release()
                card.back = nil
                announce()
                return
            } catch {
                guard !Task.isCancelled else { return }
                logger.warning("[pocket] \(key.cardId.value) could not become live: \(error)")
                try? await Task.sleep(for: pocketReopenDelay(after: attempt))
                attempt += 1
            }
        }
    }

    private func announce() {
        announced.send(live.compactMapValues { card in
            if card.refused { return false }
            return card.front == nil ? nil : true
        })
    }
}
