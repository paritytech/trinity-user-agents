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

/// A `polkadotapp://pocket/<action>?product=<id>&card=<id>` link as the core
/// classifies it. `productHost` is the product's lower-cased dotNS name.
struct PocketDeeplink: Equatable {
    let productHost: String
    let action: PocketLinkAction
    /// Already screened, by the core, as part of classifying the link. Nothing
    /// downstream screens it again.
    let cardId: PocketCardId
    /// The normalized `polkadotapp://` form, with the product and card
    /// percent-encoded, so re-parsing it names the same card.
    let canonicalUrl: String
}

/// What the host should do with a link, as the core decides it.
enum PocketLinkClassification: Equatable {
    /// A Pocket action this core serves, with its card id already screened.
    case pocket(PocketDeeplink)
    /// A bare `polkadotapp://pocket`: the collection itself, with no card named.
    case collection
    /// A Pocket link the core refused. Every link to the Pocket is the host's,
    /// so this is answered here rather than dropped.
    case malformed
    /// A link to anything other than the Pocket.
    case notOurs
}

/// Classifies through the core's `parse_navigate`, so every host reads a Pocket
/// link the same way. Kept behind this adapter, like the host bridges, so a
/// bindgen rename does not ripple through the app.
struct PocketDeeplinkParser {
    /// The host every Pocket link names.
    static let host = "pocket"
    /// The scheme the core reads a Pocket link under: the app's own, without a
    /// build flavor's suffix. A link under any of this app's schemes is read
    /// under this one.
    static let coreScheme = "polkadotapp"

    private let deeplinkMatcher: AppDeeplinkMatching

    init(deeplinkMatcher: AppDeeplinkMatching = AppDeeplinkMatcher()) {
        self.deeplinkMatcher = deeplinkMatcher
    }

    func classify(_ url: URL) -> PocketLinkClassification {
        guard
            url.host() == Self.host,
            deeplinkMatcher.isAppDeeplink(url),
            var components = URLComponents(url: url, resolvingAgainstBaseURL: false)
        else {
            return .notOurs
        }

        components.scheme = Self.coreScheme
        guard let link = components.string else {
            return .notOurs
        }

        switch parseNavigate(input: link) {
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
        case .pocketCollection:
            return .collection
        case .reject:
            return .malformed
        default:
            return .notOurs
        }
    }
}
