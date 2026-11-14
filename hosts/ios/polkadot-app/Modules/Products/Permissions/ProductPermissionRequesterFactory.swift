import Foundation
import Products
import TrUAPIHost

enum ProductPermissionRequesterFactory {
    static func create(
        router: ProductPermissionRouting,
        fundingProvider: FundingDomainProviding
    ) -> ProductPermissionRequesting {
        let requester = ProductPermissionRequester(router: router)
        // Ordered narrowest first. The trusted wrapper grants remote access and
        // notification app consent, never OS permission. It still applies when
        // the settings-screen build flag makes `ProductAutoAllowList` empty.
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
