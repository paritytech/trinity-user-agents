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

    /// The bundled products that serve Funding, as the core takes them: the
    /// manifest the app ships is the one it goes by, with no dotNS read.
    var fundingProviders: [FundingProviderEntry] {
        all.compactMap { product in
            guard product.hasWorker(), let manifest = product.workerManifest(), servesFunding(manifest) else {
                return nil
            }

            return FundingProviderEntry(productId: product.id, workerManifest: manifest, bundled: true)
        }
    }

    private func servesFunding(_ manifest: String) -> Bool {
        guard let object = try? JSONSerialization.jsonObject(with: Data(manifest.utf8)) as? [String: Any],
              let includes = object["includes"] as? [String: Any]
        else { return false }

        return includes["funding"] != nil
    }
}
