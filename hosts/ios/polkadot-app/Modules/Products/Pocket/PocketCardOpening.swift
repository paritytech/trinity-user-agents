import Foundation
import Products
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
            let page = flowState.hostProvider.page(url: url),
            let product = pocket.cardHosts.product(for: card.key, make: { surface in
                makeView(for: card, page: page, surface: surface, flowState: flowState)
            })
        else { return }

        let screen = PocketCardScreenViewController(card: card, product: product.view, surface: product.surface)
        navigator.presentFullScreen(screen)
        product.surface.claim(screen)

        Task {
            await PocketCardFaceOnOpen.apply(
                to: screen,
                for: card.key,
                cards: PublishedPocketCards.makeDefault(products: flowState.productResolver)
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

private extension PocketCardOpening {
    static func makeView(
        for card: PocketCardViewModel,
        page: ProductPage,
        surface: PocketCardSurface,
        flowState: SPAFlowState
    ) -> SPAViewProtocol? {
        let widgetURL = debugWidgetURL(for: card.key)
        let configuration = SPAConfiguration(
            title: card.title,
            isRootScreen: false,
            showMoreButton: false,
            page: page,
            contentSource: widgetURL.map(SPAContentSource.directURL) ?? .dotNs,
            cardFace: surface
        )

        // The native runtime cannot load a page by its address.
        return widgetURL == nil
            ? SPAViewFactory.createView(configuration: configuration, flowState: flowState)
            : SPAViewFactory.createRustView(configuration: configuration, flowState: flowState)
    }

    static func debugWidgetURL(for key: PocketCardKey) -> URL? {
        #if DEBUG
            return DebugPocketCards().widgetURL(for: key)
        #else
            return nil
        #endif
    }
}
