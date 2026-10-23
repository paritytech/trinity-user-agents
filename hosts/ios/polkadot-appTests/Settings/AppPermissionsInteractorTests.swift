import Foundation
import os
import Products
import Testing
@testable import polkadot_app

struct AppPermissionsInteractorTests {
    @Test
    func revokingClearsTheNativeStoreAndTheCore() async throws {
        let repository = RecordingPermissionRepository()
        let resetter = RecordingPermissionResetter()
        let interactor = makeInteractor(repository: repository, resetter: resetter)

        interactor.revokeOnDisappear(permissions: [.balanceAccess, .deviceCapability(.camera)])
        try await waitUntil { resetter.resets.count == 1 }

        #expect(repository.revoked == [[.balanceAccess, .deviceCapability(.camera)]])
        #expect(resetter.resets.first?.productId == "game.paseo")
        #expect(resetter.resets.first?.permissions == [.balanceAccess, .deviceCapability(.camera)])
    }

    @Test
    func stillClearsTheCoreWhenTheNativeRevokeFails() async throws {
        let repository = RecordingPermissionRepository()
        repository.revokeError = TestError()
        let resetter = RecordingPermissionResetter()
        let interactor = makeInteractor(repository: repository, resetter: resetter)

        interactor.revokeOnDisappear(permissions: [.webRtcAccess])
        try await waitUntil { resetter.resets.count == 1 }

        #expect(resetter.resets.first?.permissions == [.webRtcAccess])
    }

    @Test
    func revokingNothingTouchesNeither() async throws {
        let repository = RecordingPermissionRepository()
        let resetter = RecordingPermissionResetter()
        let interactor = makeInteractor(repository: repository, resetter: resetter)

        interactor.revokeOnDisappear(permissions: [])
        try await Task.sleep(for: .milliseconds(100))

        #expect(repository.revoked.isEmpty)
        #expect(resetter.resets.isEmpty)
    }
}

private struct TestError: Error {}

private func makeInteractor(
    repository: RecordingPermissionRepository,
    resetter: RecordingPermissionResetter
) -> AppPermissionsInteractor {
    AppPermissionsInteractor(
        productId: "game.paseo",
        providerFactory: IdleGrantsProvider(),
        repository: repository,
        permissionResetter: resetter,
        notificationScheduler: IdleNotificationScheduler()
    )
}

private func waitUntil(_ condition: @Sendable () -> Bool) async throws {
    for _ in 0 ..< 200 where !condition() {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(condition())
}

private struct Reset: Equatable {
    let productId: ProductId
    let permissions: [ProductPermission]
}

private final class RecordingPermissionResetter: ProductPermissionResetting, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: [Reset]())

    var resets: [Reset] {
        state.withLock { $0 }
    }

    func reset(productId: ProductId, permissions: [ProductPermission]) throws {
        state.withLock { $0.append(Reset(productId: productId, permissions: permissions)) }
    }
}

private final class RecordingPermissionRepository: ProductPermissionRepositoryProtocol, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: [[ProductPermission]]())
    var revokeError: Error?

    var revoked: [[ProductPermission]] {
        state.withLock { $0 }
    }

    func revoke(productId _: String, permissions: [ProductPermission]) async throws {
        if let revokeError { throw revokeError }
        state.withLock { $0.append(permissions) }
    }

    func revoke(productId: String, permission: ProductPermission) async throws {
        try await revoke(productId: productId, permissions: [permission])
    }

    func getPermissionState(
        productId _: String,
        permission _: ProductPermission
    ) async throws -> ProductPermissionState {
        .notDetermined
    }

    func isAnyAlwaysGranted(productId _: String, typeName _: String, keys _: [String]) async throws -> Bool { false }
    func grant(productId _: String, permission _: ProductPermission) async throws {}
    func grantOneTime(productId _: String, permission _: ProductPermission) {}
    func consumeOneTimeGrant(productId _: String, permission _: ProductPermission) -> Bool { false }
    func revokeAllByProduct(productId _: String) async throws {}
    func deny(productId _: String, permission _: ProductPermission) async throws {}
    func getAllByProduct(productId _: String) async throws -> [ProductPermissionGrant] { [] }
}

private final class IdleGrantsProvider: ProductPermissionDataProviderMaking {
    func subscribePermissionGrantsSnapshot(
        for _: NSPredicate?,
        deliverOn _: DispatchQueue,
        update _: @escaping ([ProductPermissionGrant]) -> Void,
        failure _: @escaping (Error) -> Void
    ) -> AnyObject {
        NSObject()
    }
}

private struct IdleNotificationScheduler: ProductNotificationScheduling {
    func schedule(productId _: ProductId, request _: ScheduledNotificationRequest) async throws -> UInt32 { 0 }
    func cancel(productId _: ProductId, notificationId _: UInt32) async throws {}
    func cancelAll(forProductId _: ProductId) async throws {}
}
