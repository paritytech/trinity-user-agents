import Foundation
import Products
import TrUAPIHost

enum ProductPermissionRequesterFactory {
    static func create(
        router: ProductPermissionRouting,
        stack: Stack? = TrUAPIBundledStack.opened
    ) -> ProductPermissionRequesting {
        // `hasTrustedRemotePermissions` is the core's own answer for the stack it
        // runs, so the app and the protocol path cannot disagree about which
        // products are trusted.
        TrustedRemoteProductPermissionRequester(
            isTrustedForRemoteAccess: { productId in
                stack?.hasTrustedRemotePermissions(productId: productId) ?? false
            },
            wrapped: ProductPermissionRequester(router: router)
        )
    }
}
