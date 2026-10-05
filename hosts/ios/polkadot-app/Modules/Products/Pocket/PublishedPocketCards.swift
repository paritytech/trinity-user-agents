import Foundation
import Products

/// Looks a card up in the worker manifest of the product that claims to back it.
///
/// Nothing here trusts the caller's spelling of the product: the resolver settles
/// on one canonical id, and that is the id the card is keyed by.
struct PublishedPocketCards: PublishedPocketCardsResolving {
    private let products: any ProductResolving
    private let debugCards: @Sendable (ProductId) -> [PocketCardDefinition]

    init(
        products: any ProductResolving,
        debugCards: @escaping @Sendable (ProductId) -> [PocketCardDefinition] = { _ in [] }
    ) {
        self.products = products
        self.debugCards = debugCards
    }

    func find(productId: ProductId, cardId: PocketCardId) async throws -> PublishedPocketCard {
        let resolved: ResolvedProduct
        do {
            resolved = try await products.resolve(productId)
        } catch {
            // A card typed in by hand needs no chain presence and stands either
            // way.
            if let byHand = byHand(cardId, of: productId, resolved: nil) { return byHand }

            // A manifest nothing can read is the product's answer and settles
            // the question. Anything else is a read that did not land, and is
            // raised as itself so the caller asks again.
            guard case ProductResolutionError.malformedManifest = error else { throw error }

            throw PocketPublishError.unreadableProduct
        }

        if let worker = resolved.executables.worker, worker.serves(.pocket) {
            guard let definition = worker.pocketCards.first(where: { $0.id == cardId }) else {
                throw PocketPublishError.unknownCard
            }

            return PublishedPocketCard(
                productId: resolved.id,
                productName: resolved.displayName,
                workerContentId: resolved.contentId(for: .worker),
                definition: definition
            )
        }

        // Only reached when the product publishes no Pocket worker: a published
        // one always wins, so a card supplied by hand can never shadow what a
        // product actually ships.
        guard let byHand = byHand(cardId, of: productId, resolved: resolved) else {
            throw PocketPublishError.noPocket
        }

        return byHand
    }

    private func byHand(
        _ cardId: PocketCardId,
        of productId: ProductId,
        resolved: ResolvedProduct?
    ) -> PublishedPocketCard? {
        guard let definition = debugCards(productId).first(where: { $0.id == cardId }) else { return nil }

        return PublishedPocketCard(
            productId: resolved?.id ?? productId,
            productName: resolved?.displayName ?? productId,
            workerContentId: resolved?.contentId(for: .worker) ?? productId,
            definition: definition
        )
    }
}

extension PublishedPocketCards {
    /// The resolver the app uses: published manifests, with cards typed into the
    /// debug menu filling in for products that publish no worker.
    static func makeDefault(products: any ProductResolving) -> PublishedPocketCards {
        #if DEBUG
            let debug = DebugPocketCards()

            return PublishedPocketCards(products: products, debugCards: { debug.cards(for: $0) })
        #else
            return PublishedPocketCards(products: products)
        #endif
    }
}
