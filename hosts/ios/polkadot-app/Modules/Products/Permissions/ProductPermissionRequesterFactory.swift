import Foundation
import Products
import TrUAPIHost

enum ProductPermissionRequesterFactory {
    static func create(
        router: ProductPermissionRouting,
        fundingProvider: FundingDomainProviding
    ) -> ProductPermissionRequesting {
        let requester = ProductPermissionRequester(router: router)
        // Ordered narrowest first. The trusted wrapper grants remote access only
        // and is not tied to the settings-screen build flag, so it still applies
        // in builds where `ProductAutoAllowList` is empty.
        //
        // `hasTrustedRemotePermissions` is the core's own answer, so the app and
        // the protocol path cannot disagree about which products are trusted.
        let trusted = TrustedRemoteProductPermissionRequester(
            isTrustedForRemoteAccess: { hasTrustedRemotePermissions(productId: $0) },
            wrapped: requester
        )
        return AutoAllowProductPermissionRequester(
            allowedLabels: ProductAutoAllowList.labels(fundingProvider: fundingProvider),
            wrapped: trusted
        )
    }
}
