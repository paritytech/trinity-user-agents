import Foundation
import Products
import TrUAPIHost

/// Named to avoid the core's own `PocketDeeplinkAction`: a file importing both
/// cannot tell them apart, and the collision crashes the type checker rather
/// than producing a diagnostic.
enum PocketLinkAction: Equatable {
    case add
    case open
}

/// A `/-/pocket/<action>?card=<id>` link as the core classifies it.
/// `productHost` is the lower-cased dotNS name.
struct PocketDeeplink: Equatable {
    let productHost: String
    let action: PocketLinkAction
    /// Already screened, by the core, as part of classifying the link. Nothing
    /// downstream screens it again.
    let cardId: PocketCardId
    /// The normalized `polkadot://` form, with the card id percent-encoded, so
    /// re-parsing it names the same card.
    let canonicalUrl: String
}

/// What the host should do with a link, as the core decides it.
enum PocketLinkClassification: Equatable {
    /// A Pocket action this core serves, with its card id already screened.
    case pocket(PocketDeeplink)
    /// A link the core refused. The reserved `-` target belongs to the host, so
    /// this is answered here rather than left to open a page nobody asked for.
    case malformed
    /// Anything else, including a Pocket action this core does not serve, which
    /// the core routes to the product's App so newer links degrade.
    case notOurs
}

/// Classifies through the core's `parse_navigate`, so every host reads a Pocket
/// link the same way. Kept behind this adapter, like the host bridges, so a
/// bindgen rename does not ripple through the app.
struct PocketDeeplinkParser {
    func classify(_ url: String) -> PocketLinkClassification {
        switch parseNavigate(input: url) {
        case let .pocket(identifier, action, cardId, canonicalUrl):
            let linkAction: PocketLinkAction =
                switch action {
                case .add: .add
                case .open: .open
                }

            return .pocket(PocketDeeplink(
                productHost: identifier,
                action: linkAction,
                cardId: PocketCardId(value: cardId),
                canonicalUrl: canonicalUrl
            ))
        case .reject:
            return .malformed
        default:
            return .notOurs
        }
    }
}
