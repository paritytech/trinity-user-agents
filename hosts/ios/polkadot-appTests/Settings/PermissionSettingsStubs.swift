import Foundation
import os
import AsyncExtensions
import Products
@testable import polkadot_app

/// Answers the core's stored grants from a stub and records every reset.
final class RecordingCoreAuthorizations: TrUAPIPermissionAuthorizing, @unchecked Sendable {
    struct Reset: Equatable {
        let productId: ProductId
        let permissions: [ProductPermission]
    }

    private let subject = AsyncPassthroughSubject<ProductId>()
    private let state = OSAllocatedUnfairLock(initialState: (resets: [Reset](), reads: 0))

    var grantsToReturn: [ProductPermissionGrant] = []
    var grantsError: Error?

    var changes: AnyAsyncSequence<ProductId> {
        subject.eraseToAnyAsyncSequence()
    }

    var resets: [Reset] {
        state.withLock { $0.resets }
    }

    var reads: Int {
        state.withLock { $0.reads }
    }

    func grants() async throws -> [ProductPermissionGrant] {
        state.withLock { $0.reads += 1 }
        if let grantsError { throw grantsError }
        return grantsToReturn
    }

    func reset(productId: ProductId, permissions: [ProductPermission]) throws {
        state.withLock { $0.resets.append(Reset(productId: productId, permissions: permissions)) }
    }

    func changed(productId: ProductId) {
        subject.send(productId)
    }
}

/// Delivers one native snapshot to every subscriber, like the store would.
final class SnapshotGrantsProvider: ProductPermissionDataProviderMaking {
    private let grants: [ProductPermissionGrant]

    init(grants: [ProductPermissionGrant] = []) {
        self.grants = grants
    }

    func subscribePermissionGrantsSnapshot(
        for predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([ProductPermissionGrant]) -> Void,
        failure _: @escaping (Error) -> Void
    ) -> AnyObject {
        let matching = grants.filter { grant in
            predicate?.evaluate(with: NativeGrantRecord(grant)) ?? true
        }
        queue.async { update(matching) }
        return NSObject()
    }
}

/// The attributes the native store's predicates read, so a stub can filter like the store does.
private final class NativeGrantRecord: NSObject {
    @objc let productId: String
    @objc let granted: Bool

    init(_ grant: ProductPermissionGrant) {
        productId = grant.productId
        granted = grant.granted
    }
}

func waitUntil(
    timeout: Duration = .seconds(2),
    _ condition: @Sendable () async -> Bool
) async throws -> Bool {
    let deadline = ContinuousClock.now + timeout
    while ContinuousClock.now < deadline {
        if await condition() { return true }
        try await Task.sleep(for: .milliseconds(10))
    }
    return await condition()
}

func grant(_ productId: ProductId, _ permission: ProductPermission) -> ProductPermissionGrant {
    ProductPermissionGrant(productId: productId, permission: permission, granted: true, grantedAt: nil)
}
