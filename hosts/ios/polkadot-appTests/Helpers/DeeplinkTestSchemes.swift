import Foundation

@testable import polkadot_app

/// Build-flavor deeplink schemes for tests that must hold under any configuration.
enum DeeplinkTestSchemes {
    static let active = AppConfig.DeepLink.scheme
    /// A non-active flavor scheme of the CURRENT brand, synthesized from the brand base so
    /// it stays correct under any brand and any set of build configurations. No real
    /// configuration uses this suffix, so it can never collide with `active`.
    static let other = AppConfig.Brand.deeplinkBase + "othertest"
}
