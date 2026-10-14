import Foundation
import CoreData
import AsyncExtensions
import StructuredConcurrency
import Products

extension ProductPermissionDataProviderMaking {
    func subscribeGrants(
        productId: ProductId,
        grantedOnly: Bool = true
    ) -> AnyAsyncSequence<[ProductPermissionGrant]> {
        var predicates: [NSPredicate] = [.permissionGrant(productId: productId)]

        if grantedOnly {
            predicates.append(.permissionGrantGrantedOnly())
        }

        let predicate = NSCompoundPredicate(andPredicateWithSubpredicates: predicates)

        return subscribeGrantsWithPredicate(predicate)
    }

    func subscribeAllGrants(
        grantedOnly: Bool
    ) -> AnyAsyncSequence<[ProductPermissionGrant]> {
        let predicate: NSPredicate? = grantedOnly ? .permissionGrantGrantedOnly() : nil
        return subscribeGrantsWithPredicate(predicate)
    }

    func subscribeGrantsWithPredicate(
        _ predicate: NSPredicate?
    ) -> AnyAsyncSequence<[ProductPermissionGrant]> {
        let syncQueue = DispatchQueue(label: "io.products.permissions.provider.async.updates")

        return AsyncThrowingStream { continuation in
            let holder = AnyObjectHolder<AnyObject>()

            let provider = subscribePermissionGrantsSnapshot(
                for: predicate,
                deliverOn: syncQueue,
                update: { grants in
                    continuation.yield(grants)
                },
                failure: { error in
                    continuation.yield(with: .failure(error))
                }
            )

            holder.set(provider)

            continuation.onTermination = { @Sendable _ in
                holder.set(nil)
            }
        }
        .eraseToAnyAsyncSequence()
    }
}
