import Foundation
import Products
import UIKit

/// Opens the card's product, at the page the card names, on a screen the card
/// itself heads.
///
/// An expanded card runs the product's app, the one executable every product
/// publishes. The origin stays the product's base domain, so the card sees the
/// same grants and storage its worker does.
@MainActor
struct PocketCardOpening {
    let flowState: SPAFlowState
    let navigator: ModuleNavigating
    let pocket: ProductPocketService

    func open(_ card: PocketCardViewModel, pageQuery: String = "") {
        guard
            let url = card.key.launchUrl(pageQuery: pageQuery),
            let page = flowState.hostProvider.page(url: url)
        else { return }

        navigator.openWallet { wallet, wasOnScreen in
            // A second screen would take the open card's page and leave it blank.
            guard !pocket.cardHosts.isOnDisplay(card.key) else {
                let widgetURL = debugWidgetURL(for: card.key, pageQuery: pageQuery)
                pocket.cardHosts.reload(card.key, at: pageQuery) { view in
                    if let widgetURL { view.navigate(to: widgetURL) } else { view.navigate(to: page) }
                }
                return
            }

            guard let product = pocket.cardHosts.product(for: card.key, pageQuery: pageQuery, make: { _ in
                makeView(for: card, page: page, pageQuery: pageQuery)
            }) else { return }

            let origin = wallet.viewControllers.first as? PocketOrigin
            let screen = PocketCardScreenViewController(card: card, product: product.view, origin: origin)
            PocketCardNavigation.show(screen, on: wallet, animated: wasOnScreen) {
                product.surface.claim(screen)
            }

            Task { [weak screen] in
                let shown = await PocketCardFaceOnOpen.faceShown(
                    for: card.key,
                    cards: PublishedPocketCards.makeDefault(products: flowState.productResolver)
                )
                screen?.applyOpeningFace(shown: shown)
            }
        }
    }

    /// A link may name a card the Pocket does not hold, which is the one case
    /// the user is told about rather than silently taken into the product.
    func open(link: PocketDeeplink) {
        let key = PocketCardKey(productId: link.productHost, cardId: link.cardId)

        Task { @MainActor in
            guard let card = await PocketCardsProvider(store: pocket.collection).card(for: key) else {
                PocketRefusalPresenter.show(String(localized: .Products.pocketDeeplinkUnknownCard))
                return
            }

            open(card, pageQuery: link.pageQuery)
        }
    }
}

private extension PocketCardOpening {
    func makeView(
        for card: PocketCardViewModel,
        page: ProductPage,
        pageQuery: String
    ) -> SPAViewProtocol? {
        let widgetURL = debugWidgetURL(for: card.key, pageQuery: pageQuery)
        let configuration = SPAConfiguration(
            title: card.title,
            isRootScreen: false,
            showMoreButton: false,
            page: page,
            contentSource: widgetURL.map(SPAContentSource.directURL) ?? .dotNs
        )

        // A page loaded by its address goes to the Rust runtime on its own.
        return SPAViewFactory.createView(configuration: configuration, flowState: flowState)
    }

    func debugWidgetURL(for key: PocketCardKey, pageQuery: String) -> URL? {
        #if DEBUG
            return DebugPocketCards().widgetURL(for: key, pageQuery: pageQuery)
        #else
            return nil
        #endif
    }
}
