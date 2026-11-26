import Foundation
import FoundationExt
import IssueMonitoring
import Products
import TrUAPIHost
import UIKit

/// Opens the card's product, at the page the card names, on a screen the card
/// itself heads.
///
/// An expanded card runs the product's widget, or its app when it publishes no
/// widget. The origin stays the product's base domain, so the card sees the
/// same grants and storage its worker does.
@MainActor
struct PocketCardOpening {
    let flowState: SPAFlowState
    let navigator: ModuleNavigating
    let pocket: ProductPocketService
    /// Sends nothing unless the composition point passes `IssueMonitoringFactory.createHostEventRecorder()`.
    var hostEventRecorder: HostEventRecording = NoopHostEventRecorder()
    /// The tap's wall clock, read synchronously at the tap: an async `DateProviding` read would add a hop.
    var dateProvider: CurrentDateProviding = NowDateProvider()

    /// A card tapped in the Pocket is stamped here, on the tap's own call; a link passes the `tap` read where it
    /// arrived. Either way the page measures from the user's action, not from this call's hops.
    func open(
        _ card: PocketCardViewModel,
        pageQuery: String = "",
        tap: HostOpenTap? = nil
    ) {
        let tap = tap ?? HostOpenTap(source: .pocketCard, openedAt: dateProvider.now)

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

            guard
                let view = pocket.cardHosts.open(card.key, pageQuery: pageQuery, hostSessionID: tap.sessionID, make: {
                    makeView(for: card, page: page, pageQuery: pageQuery, tap: tap)
                }),
                let screen = view.controller as? PocketItemScreen
            else { return }
            recordOpened(tap, reused: false)

            view.origin = wallet.viewControllers.first as? PocketOrigin
            PocketNavigation.show(screen, on: wallet, animated: wasOnScreen) {}

            Task { [weak view] in
                let covered = await PocketCardCoverOnOpen.startsCovered(
                    for: card.key,
                    cards: PublishedPocketCards.makeDefault(products: flowState.productResolver)
                )
                view?.applyOpeningCover(covered: covered)
            }
        }
    }

    /// A link may name a card the Pocket does not hold, which is the one case
    /// the user is told about rather than silently taken into the product.
    /// `tap` is the link's arrival, read where the system handed it over (`PocketOpenService`).
    func open(link: PocketDeeplink, tap: HostOpenTap) {
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
    ) -> ExpandedPocketCardViewProtocol? {
        let widgetURL = debugWidgetURL(for: card.key, pageQuery: pageQuery)

        return ExpandedPocketCardViewFactory.createView(
            card: card,
            page: page,
            contentSource: widgetURL.map(SPAContentSource.directURL) ?? .dotNs,
            openTap: tap,
            flowState: flowState
        )
    }

    func debugWidgetURL(for key: PocketCardKey, pageQuery: String) -> URL? {
        #if DEBUG
            return DebugPocketCards().widgetURL(for: key, pageQuery: pageQuery)
        #else
            return nil
        #endif
    }
}
