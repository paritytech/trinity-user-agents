import Products
import TrUAPIHost

extension PocketCardScreening {
    /// The core's own screening, which is what every Pocket call and every
    /// Pocket deeplink is already measured against. Anything else here would
    /// refuse cards the core accepts, or accept ones it refuses.
    static var core: PocketCardScreening {
        PocketCardScreening(
            id: { try screenPocketCardId(id: $0) },
            title: { try screenPocketCardTitle(title: $0) }
        )
    }
}
