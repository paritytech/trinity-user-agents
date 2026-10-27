import struct TrUAPIHost.ProductId
import CoreData
import Foundation

extension NSPredicate {
    static func permissionGrant(productId: ProductId) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDProductPermissionGrant.productId),
            productId.rawValue
        )
    }

    static func permissionGrantGrantedOnly() -> NSPredicate {
        NSPredicate(
            format: "%K == YES",
            #keyPath(CDProductPermissionGrant.granted)
        )
    }
}
