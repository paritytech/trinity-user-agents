import Foundation
import Products

/// Warms the archive a host-placed card opens, before the user presses it.
///
/// Host-placed cards only. They are few and fixed, where a Pocket full of added
/// cards would turn one visit to the tab into a fetch per product, which is
/// the opposite of what this is for.
@MainActor
final class PocketPrewarmer {
    private let warmer: ProductArchiveWarmer
    private let logger: LoggerProtocol

    /// Only products whose archive is now on disk. A warm that failed left
    /// nothing there, so the next visit tries again rather than leaving the
    /// card slow for the rest of the run.
    private var warmed: Set<ProductId> = []

    init(
        products: any ProductResolving,
        dotNsResolver: any DotNsResolverProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        warmer = ProductArchiveWarmer(products: products, dotNsResolver: dotNsResolver)
        self.logger = logger
    }

    func warm(_ cards: [PocketCardViewModel]) async {
        let pending = Set(cards.filter(\.privileged).map(\.key.productId)).subtracting(warmed)

        for productId in pending {
            await warm(productId)
        }
    }

    /// The widget, because that is what a pressed card opens.
    private func warm(_ productId: ProductId) async {
        do {
            try await warmer.warm(productId, serving: .widget)
            warmed.insert(productId)
            logger.debug("[pocket] warmed \(productId)'s archive")
        } catch {
            logger.error("[pocket] could not warm \(productId)'s archive: \(error)")
        }
    }
}
