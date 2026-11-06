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
    /// manifest the app ships is the one it goes by, with no dotNS read. A
    /// product whose manifest cannot be read is left out and logged.
    func fundingProviders(logger: LoggerProtocol) -> [FundingProviderEntry] {
        all.compactMap { product in
            guard product.hasWorker() else { return nil }

            do {
                guard let manifest = try product.fundingWorkerManifest() else { return nil }

                return FundingProviderEntry(productId: product.id, workerManifest: manifest, origin: .bundled)
            } catch {
                logger.error("[funding] bundled \(product.id.rawValue) ships an unreadable worker manifest: \(error)")
                return nil
            }
        }
    }
}
