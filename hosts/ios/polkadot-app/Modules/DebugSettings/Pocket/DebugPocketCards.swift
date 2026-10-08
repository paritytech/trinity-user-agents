import Foundation
@preconcurrency import Keystore_iOS
import Products
import TrUAPIHost

/// A Pocket card supplied by hand, for a product that publishes no worker
/// manifest.
///
/// This is the only path that produces a ``PocketCardPreview/url(_:)``: a
/// published manifest can never name an address the host then fetches, and
/// this one is typed in by whoever is holding the phone.
struct DebugPocketCard: Codable, Equatable, Swift.Identifiable {
    let productId: String
    let cardId: String
    let title: String
    let faceUrl: String

    var id: String { "\(productId)/\(cardId)" }

    var definition: PocketCardDefinition? {
        // A card id that does not screen reads as no card rather than as an
        // error, so a typo shows up as the card simply not being offered.
        guard let screened = try? screenPocketCardId(id: cardId) else { return nil }

        return PocketCardDefinition(id: PocketCardId(value: screened), title: title, preview: .url(faceUrl))
    }
}

/// Cards typed into the debug menu, held across launches.
protocol DebugPocketCardsStoring: Sendable {
    func cards() -> [DebugPocketCard]

    func cards(for productId: ProductId) -> [PocketCardDefinition]

    func save(_ card: DebugPocketCard)

    func delete(_ card: DebugPocketCard)
}

struct DebugPocketCards: DebugPocketCardsStoring {
    private let settingsManager: SettingsManagerProtocol

    init(settingsManager: SettingsManagerProtocol = SettingsManager.shared) {
        self.settingsManager = settingsManager
    }

    func cards() -> [DebugPocketCard] {
        guard let stored = settingsManager.anyValue(for: SettingsKey.debugPocketCards.rawValue) as? Data else {
            return []
        }

        return (try? JSONDecoder().decode([DebugPocketCard].self, from: stored)) ?? []
    }

    func cards(for productId: ProductId) -> [PocketCardDefinition] {
        cards()
            .filter { $0.productId.lowercased() == productId.lowercased() }
            .compactMap(\.definition)
    }

    func save(_ card: DebugPocketCard) {
        var held = cards().filter { $0.id != card.id }
        held.append(card)
        write(held)
    }

    func delete(_ card: DebugPocketCard) {
        write(cards().filter { $0.id != card.id })
    }

    private func write(_ cards: [DebugPocketCard]) {
        guard let data = try? JSONEncoder().encode(cards) else { return }

        settingsManager.set(anyValue: data, for: SettingsKey.debugPocketCards.rawValue)
    }
}
