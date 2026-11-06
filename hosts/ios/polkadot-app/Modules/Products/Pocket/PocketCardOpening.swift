import Foundation
import IssueMonitoring
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
        open(card, pageQuery: pageQuery, tap: telemetry.beginOpen(source: .pocketCard))
    }

    private func open(_ card: PocketCardViewModel, pageQuery: String, tap: HostOpenTap?) {
        guard
            let url = card.key.launchUrl(pageQuery: pageQuery),
            let page = flowState.hostProvider.page(url: url)
        else { return }

        navigator.openWallet { wallet, wasOnScreen in
            // A second screen would take the open card's page and leave it blank.
            guard !pocket.cardHosts.isOnDisplay(card.key) else {
                let widgetURL = debugWidgetURL(for: card.key, pageQuery: pageQuery)
                // Not a tap that shows anything: the card is already on screen, at most loading a new page.
                pocket.cardHosts.reload(card.key, at: pageQuery) { view in
                    if let widgetURL { view.navigate(to: widgetURL) } else { view.navigate(to: page) }
                }
                return
            }

            var built = false
            let openTap = tap.map(SPAOpenTap.init)
            guard let product = pocket.cardHosts.product(
                for: card.key,
                pageQuery: pageQuery,
                openTap: openTap,
                make: { _ in
                    built = true
                    return makeView(for: card, page: page, pageQuery: pageQuery, openTap: openTap)
                }
            ) else { return }

            let reused = !built

            let origin = wallet.viewControllers.first as? PocketCardOrigin
            let screen = PocketCardScreenViewController(card: card, product: product.view, origin: origin)
            PocketCardNavigation.show(screen, on: wallet, animated: wasOnScreen) {
                product.surface.claim(screen)

                if reused {
                    PocketCardTapReporter(telemetry: telemetry).reportReused(
                        tap,
                        productId: card.key.productId,
                        hostSessionId: product.openTap?.pageSession
                    )
                }
            }

            Task { [weak screen] in
                let shown = await PocketCardFaceOnOpen.faceShown(
                    for: card.key,
                    cards: PublishedPocketCards.makeDefault(products: flowState.productResolver),
                    telemetry: telemetry
                )
                screen?.applyOpeningFace(shown: shown)
            }
        }
    }

    /// A link may name a card the Pocket does not hold, which is the one case
    /// the user is told about rather than silently taken into the product.
    func open(link: PocketDeeplink) {
        let tap = telemetry.beginOpen(source: .deeplink)
        let key = PocketCardKey(productId: link.productHost, cardId: link.cardId)

        Task { @MainActor in
            guard let card = await PocketCardsProvider(store: pocket.collection).card(for: key) else {
                PocketRefusalPresenter.show(String(localized: .Products.pocketDeeplinkUnknownCard))
                return
            }

            open(card, pageQuery: link.pageQuery, tap: tap)
        }
    }
}

private extension PocketCardOpening {
    var telemetry: HostLoadTelemetry {
        flowState.telemetry
    }

    func makeView(
        for card: PocketCardViewModel,
        page: ProductPage,
        pageQuery: String,
        openTap: SPAOpenTap?
    ) -> SPAViewProtocol? {
        let widgetURL = debugWidgetURL(for: card.key, pageQuery: pageQuery)
        let configuration = SPAConfiguration(
            title: card.title,
            isRootScreen: false,
            showMoreButton: false,
            page: page,
            contentSource: widgetURL.map(SPAContentSource.directURL) ?? .dotNs,
            openTap: openTap
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
