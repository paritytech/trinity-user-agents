import Foundation
import Products

extension BundledProducts {
    /// The products this build ships, from `BundledProducts.bundle`, which
    /// `Runscripts/bundle-funding-provider.sh` fills. Testnet builds only; a
    /// build made without the bundle ships none.
    static let app: BundledProducts = {
        #if TESTNET_FEATURE
            BundledProducts(root: Bundle.main.url(forResource: "BundledProducts", withExtension: "bundle"))
        #else
            .none
        #endif
    }()
}
