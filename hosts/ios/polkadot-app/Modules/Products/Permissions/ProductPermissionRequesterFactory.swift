import Foundation
import Products
import TrUAPIHost

enum ProductPermissionRequesterFactory {
    static func create(router: ProductPermissionRouting) -> ProductPermissionRequesting {
        // `hasTrustedRemotePermissions` is the core's own answer, so the app and
        // the protocol path cannot disagree about which products are trusted.
        TrustedRemoteProductPermissionRequester(
            isTrustedForRemoteAccess: { hasTrustedRemotePermissions(productId: $0) },
            wrapped: ProductPermissionRequester(router: router)
        )
    }
}
