import BulletinChain
import Foundation
import Products

protocol ProductIconLoading: Sendable {
    /// Icon bytes declared by the product's root manifest, or nil when it declares none the Host
    /// can render. Callers fall back to their own placeholder.
    func loadIcon(for productId: ProductId) async -> Data?
}

/// Loads product icons from the Bulletin IPFS gateway by the CID the root manifest pins.
///
/// @unchecked Sendable: immutable wiring only.
final class ProductIconLoader: ProductIconLoading, @unchecked Sendable {
    private let productResolver: ProductResolving
    private let ipfsFetcher: IpfsFetching
    private let logger: LoggerProtocol

    init(
        productResolver: ProductResolving,
        ipfsFetcher: IpfsFetching = IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productResolver = productResolver
        self.ipfsFetcher = ipfsFetcher
        self.logger = logger
    }

    // ponytail: fetched bytes are not verified against the CID — same as the Android and desktop
    // Hosts. Icons decode only through UIImage, so a substituted image spoofs branding rather than
    // executing anything. Verify here if the gateway ever stops being trusted.
    func loadIcon(for productId: ProductId) async -> Data? {
        do {
            guard let icon = try await productResolver.resolve(productId).icon else {
                return nil
            }

            return try await ipfsFetcher.lookupBy(cid: icon.cid)
        } catch {
            logger.error("Failed to load manifest icon for \(productId): \(error)")
            return nil
        }
    }
}
