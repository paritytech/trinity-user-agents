import Foundation
import AsyncExtensions
import StructuredConcurrency
import Products

extension ProductPermissionDataProviderMaking {
    func subscribeGrants(
        productId: ProductId,
        grantedOnly: Bool = true
    ) -> AnyAsyncSequence<[ProductPermissionGrant]> {
        subscribeCanonicalGrants(productId: productId, grantedOnly: grantedOnly)
    }

    func subscribeAllGrants(
        grantedOnly: Bool
    ) -> AnyAsyncSequence<[ProductPermissionGrant]> {
        subscribeCanonicalGrants(productId: nil, grantedOnly: grantedOnly)
    }

    private func subscribeCanonicalGrants(
        productId: ProductId?,
        grantedOnly: Bool
    ) -> AnyAsyncSequence<[ProductPermissionGrant]> {
        let queue = DispatchQueue(label: "io.products.permissions.canonical.updates")
        let repository = permissionRepository
        return AsyncThrowingStream { continuation in
            // Both stores trigger a fresh canonical read, never a merge of a
            // stale native snapshot with a newer legacy snapshot.
            let updates = AsyncThrowingStream<[ProductPermissionGrant]?, Error> { updates in
                let providerHolder = AnyObjectHolder<AnyObject>()
                let observerHolder = AnyObjectHolder<NSObjectProtocol>()
                let provider = subscribePermissionGrantsSnapshot(
                    for: nil, deliverOn: queue,
                    update: { updates.yield($0) },
                    failure: { updates.finish(throwing: $0) }
                )
                providerHolder.set(provider)
                let observer = NotificationCenter.default.addObserver(
                    forName: .productPermissionAuthorizationsChanged, object: nil, queue: nil
                ) { notification in
                    if let productId, let changed = notification.object as? String,
                       ProductPermission.bareProductLabel(changed) != ProductPermission.bareProductLabel(productId) {
                        return
                    }
                    updates.yield(nil)
                }
                observerHolder.set(observer)
                updates.yield(nil)
                updates.onTermination = { @Sendable _ in
                    providerHolder.set(nil)
                    if let observer = observerHolder.get() {
                        NotificationCenter.default.removeObserver(observer)
                    }
                }
            }
            let task = Task {
                var legacy: [ProductPermissionGrant] = []
                do {
                    for try await snapshot in updates {
                        if let snapshot { legacy = snapshot }
                        let grants: [ProductPermissionGrant]
                        if let productId {
                            grants = try await repository.getAllByProduct(productId: productId)
                        } else {
                            grants = try await repository.settingsGrants(legacy: legacy)
                        }
                        continuation.yield(grantedOnly ? grants.filter(\.granted) : grants)
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { @Sendable _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }
}
