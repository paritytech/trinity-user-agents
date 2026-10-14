import CoreData
import Foundation

extension NSPredicate {
    static func permissionGrant(productId: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDProductPermissionGrant.productId),
            productId
        )
    }

    static func permissionGrantGrantedOnly() -> NSPredicate {
        NSPredicate(
            format: "%K == YES",
            #keyPath(CDProductPermissionGrant.granted)
        )
    }
}
