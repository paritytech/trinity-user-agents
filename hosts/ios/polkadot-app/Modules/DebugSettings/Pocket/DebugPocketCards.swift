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
    /// The page the card opens instead of its product's widget, even a published one.
    let widgetUrl: String?
    let faceShown: Bool?

    var id: String { "\(productId)/\(cardId)" }

    var definition: PocketCardDefinition? {
        // A card id that does not screen reads as no card rather than as an
        // error, so a typo shows up as the card simply not being offered.
        guard let screened = try? screenPocketCardId(id: cardId) else { return nil }

        return PocketCardDefinition(
            id: PocketCardId(value: screened),
            title: title,
            preview: .url(faceUrl),
            faceShown: faceShown ?? true
        )
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
        cards(of: productId).compactMap(\.definition)
    }

    func save(_ card: DebugPocketCard) {
        var held = cards().filter { $0.id != card.id }
        held.append(card)
        write(held)
    }

    func delete(_ card: DebugPocketCard) {
        write(cards().filter { $0.id != card.id })
    }

    /// The page is loaded as typed rather than through the card's launch
    /// address, so it is handed the launch address's query in place of any
    /// typed item of the same name, with the typed encoding kept as it is.
    func widgetURL(for key: PocketCardKey) -> URL? {
        guard
            let widgetUrl = cards(of: key.productId).first(where: { $0.definition?.id == key.cardId })?.widgetUrl,
            var components = URLComponents(string: widgetUrl),
            let launchUrl = key.launchUrl,
            let launchItems = URLComponents(url: launchUrl, resolvingAgainstBaseURL: false)?.percentEncodedQueryItems
        else { return nil }

        let launchNames = Set(launchItems.map(\.name))
        let typedItems = (components.percentEncodedQueryItems ?? []).filter { !launchNames.contains($0.name) }
        components.percentEncodedQueryItems = typedItems + launchItems

        return components.url
    }

    private func write(_ cards: [DebugPocketCard]) {
        guard let data = try? JSONEncoder().encode(cards) else { return }

        settingsManager.set(anyValue: data, for: SettingsKey.debugPocketCards.rawValue)
    }
}

private extension DebugPocketCards {
    func cards(of productId: ProductId) -> [DebugPocketCard] {
        cards().filter { $0.productId.lowercased() == productId.lowercased() }
    }
}
