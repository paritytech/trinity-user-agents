import Foundation
import Products
import TrUAPIHost

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

    /// The bundled products that serve Funding, as the core finds them in
    /// the bundle: each Worker whose shipped manifest includes Funding, with
    /// that manifest and whether it ships an App. The core logs and leaves
    /// out a manifest it cannot read.
    func fundingProviders() -> [FundingProviderEntry] {
        guard let root else { return [] }

        return bundledFundingProviders(root: root.path, reader: FileBundleReader())
    }
}
