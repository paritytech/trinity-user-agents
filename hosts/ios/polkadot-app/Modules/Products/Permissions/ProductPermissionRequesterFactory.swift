import Foundation
import Products
import TrUAPIHost

enum ProductPermissionRequesterFactory {
    static func create(
        router: ProductPermissionRouting,
        fundingProvider: FundingDomainProviding,
        stack: Stack? = TrUAPIBundledStack.opened
    ) -> ProductPermissionRequesting {
        let requester = ProductPermissionRequester(router: router)
        // Ordered narrowest first. The trusted wrapper grants remote access only
        // and is not tied to the settings-screen build flag, so it still applies
        // in builds where `ProductAutoAllowList` is empty.
        //
        // `hasTrustedRemotePermissions` is the core's own answer for the stack it
        // runs, so the app and the protocol path cannot disagree about which
        // products are trusted.
        let trusted = TrustedRemoteProductPermissionRequester(
            isTrustedForRemoteAccess: { productId in
                stack?.hasTrustedRemotePermissions(productId: productId) ?? false
            },
            wrapped: requester
        )
        return AutoAllowProductPermissionRequester(
            allowedLabels: ProductAutoAllowList.labels(fundingProvider: fundingProvider),
            wrapped: trusted
        )
    }
}
