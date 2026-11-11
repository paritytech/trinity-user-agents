import Foundation
import IssueMonitoring
import Products
import TrUAPIHost
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
    var hostEventRecorder: HostEventRecording = IssueMonitoringFactory.createHostEventRecorder()

    /// `tap` is read where the user acted, so the page measures from the tap, not from this call's hops.
    func open(
        _ card: PocketCardViewModel,
        pageQuery: String = "",
        tap: HostOpenTap = HostOpenTap(source: .pocketCard)
    ) {
        guard
            let url = card.key.launchUrl(pageQuery: pageQuery),
            let page = flowState.hostProvider.page(url: url)
        else { return }

        navigator.openWallet { wallet, wasOnScreen in
            // A second screen would take the open card's page and leave it blank.
            guard !pocket.cardHosts.isOnDisplay(card.key) else {
                let heldSessionID = pocket.cardHosts.hostSessionID(for: card.key)
                recordOpened(heldSessionID.map(tap.showing(sessionID:)) ?? tap, reused: true)
                let widgetURL = debugWidgetURL(for: card.key, pageQuery: pageQuery)
                pocket.cardHosts.reload(card.key, at: pageQuery) { view in
                    if let widgetURL { view.navigate(to: widgetURL) } else { view.navigate(to: page) }
                }
                return
            }

            // A product the Pocket still holds is shown again without a new page, so the tap joins the session of
            // the page it shows; a page built here adopts the tap's own id.
            var builtPage = false
            guard let product = pocket.cardHosts.product(
                for: card.key,
                pageQuery: pageQuery,
                hostSessionID: tap.sessionID,
                make: { _ in
                    builtPage = true
                    return makeView(for: card, page: page, pageQuery: pageQuery, tap: tap)
                }
            ) else { return }
            recordOpened(tap.showing(sessionID: product.hostSessionID), reused: !builtPage)

            let origin = wallet.viewControllers.first as? PocketCardOrigin
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
        let tap = HostOpenTap(source: .deeplink)
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
    func recordOpened(_ tap: HostOpenTap, reused: Bool) {
        hostEventRecorder.record(.pocketOpened(
            tap: tap,
            reused: reused,
            origin: .current,
            truapiHostVersion: truapiHostVersion(),
            monotonicMilliseconds: HostEventClock.monotonicMilliseconds()
        ))
    }

    func makeView(
        for card: PocketCardViewModel,
        page: ProductPage,
        pageQuery: String,
        tap: HostOpenTap
    ) -> SPAViewProtocol? {
        let widgetURL = debugWidgetURL(for: card.key, pageQuery: pageQuery)
        let configuration = SPAConfiguration(
            title: card.title,
            isRootScreen: false,
            showMoreButton: false,
            page: page,
            contentSource: widgetURL.map(SPAContentSource.directURL) ?? .dotNs,
            openTap: tap
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
