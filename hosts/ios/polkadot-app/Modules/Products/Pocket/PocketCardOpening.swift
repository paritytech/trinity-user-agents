import Foundation
import Products
import StructuredConcurrency
import UIKit

/// Opens the card's product, at the page the card names, on a screen the card
/// itself heads.
///
/// The protocol names the widget as what an expanded card runs, so that is the
/// executable served here. The origin stays the product's base domain, so the
/// card sees the same grants and storage its worker does.
@MainActor
enum PocketCardOpening {
    static func open(
        _ card: PocketCardViewModel,
        flowState: SPAFlowState,
        navigator: ModuleNavigating,
        pocket: ProductPocketService
    ) {
        guard
            let url = card.key.launchUrl,
            let page = flowState.hostProvider.page(url: url)
        else { return }

        Task { @MainActor in
            let faceShown = await PocketCardFaceOnOpen.faceShown(
                for: card.key,
                cards: PublishedPocketCards.makeDefault(products: flowState.productResolver)
            )

            let product = pocket.cardHosts.product(for: card.key) { surface in
                let configuration = SPAConfiguration(
                    title: card.title,
                    isRootScreen: false,
                    showMoreButton: false,
                    page: page,
                    executable: .widget,
                    cardSurface: surface
                )

                return SPAViewFactory.createView(configuration: configuration, flowState: flowState)
            }

            guard let product else { return }

            navigator.presentFullScreen(
                PocketCardScreenViewController(
                    card: card,
                    product: product.view,
                    surface: product.surface,
                    faceShown: faceShown
                )
            )
        }
    }

    /// A link may name a card the Pocket does not hold, which is the one case
    /// the user is told about rather than silently taken into the product.
    static func open(
        link: PocketDeeplink,
        flowState: SPAFlowState,
        navigator: ModuleNavigating,
        pocket: ProductPocketService
    ) {
        let key = PocketCardKey(productId: link.productHost, cardId: link.cardId)

        Task { @MainActor in
            guard let card = await PocketCardsProvider(store: pocket.collection).card(for: key) else {
                PocketRefusalPresenter.show(String(localized: .Products.pocketDeeplinkUnknownCard))
                return
            }

            open(card, flowState: flowState, navigator: navigator, pocket: pocket)
        }
    }
}

/// Whether a card opens with its face shown, as its product published it.
enum PocketCardFaceOnOpen {
    /// A card whose product cannot be asked in time opens with its face shown,
    /// since a face hidden by mistake is not one the user knows to pull back.
    static func faceShown(
        for key: PocketCardKey,
        cards: any PublishedPocketCardsResolving,
        timeout: Duration = .milliseconds(500)
    ) async -> Bool {
        let published = try? await withTimeout(timeout) {
            try await cards.find(productId: key.productId, cardId: key.cardId).definition.faceShown
        }

        return published ?? true
    }
}
