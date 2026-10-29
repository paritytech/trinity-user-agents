import Foundation
import os
import Products
import Testing
@testable import polkadot_app

struct AppPermissionsInteractorTests {
    @Test
    func listsTheCoresGrantsBesideTheNativeOnes() async throws {
        let core = RecordingCoreAuthorizations()
        core.grantsToReturn = [
            grant("other.paseo", .balanceAccess),
            grant("game.paseo", .networkAccess(domain: "api.example.com")),
            grant("game.paseo", .deviceCapability(.camera))
        ]
        let (interactor, presenter) = await makeInteractor(
            nativeGrants: [grant("game.paseo", .deviceCapability(.camera)), grant("game.paseo", .webRtcAccess)],
            core: core
        )

        interactor.setup()
        let listed = try await waitUntil { await presenter.lastGrants.count == 3 }

        let expected = [
            grant("game.paseo", .deviceCapability(.camera)),
            grant("game.paseo", .webRtcAccess),
            grant("game.paseo", .networkAccess(domain: "api.example.com"))
        ]
        #expect(listed)
        #expect(await presenter.lastGrants == expected)
    }

    @Test
    func readsTheCoreAgainWhenItsDecisionsOnThisProductMove() async throws {
        let core = RecordingCoreAuthorizations()
        let (interactor, presenter) = await makeInteractor(core: core)

        interactor.setup()
        _ = try await waitUntil { await presenter.updates == 2 }
        core.grantsToReturn = [grant("game.paseo", .balanceAccess)]
        core.changed(productId: "other.paseo")
        core.changed(productId: "game.paseo")
        let relisted = try await waitUntil { await presenter.lastGrants == [grant("game.paseo", .balanceAccess)] }

        #expect(relisted)
        #expect(core.reads == 2)
    }

    @Test
    func revokingClearsTheNativeStoreAndTheCore() async throws {
        let repository = RecordingPermissionRepository()
        let core = RecordingCoreAuthorizations()
        let (interactor, _) = await makeInteractor(repository: repository, core: core)

        interactor.revokeOnDisappear(permissions: [.balanceAccess, .deviceCapability(.camera)])
        _ = try await waitUntil { core.resets.count == 1 }

        #expect(repository.revoked == [[.balanceAccess, .deviceCapability(.camera)]])
        #expect(core.resets == [.init(
            productId: "game.paseo",
            permissions: [.balanceAccess, .deviceCapability(.camera)]
        )])
    }

    @Test
    func stillClearsTheCoreWhenTheNativeRevokeFails() async throws {
        let repository = RecordingPermissionRepository()
        repository.revokeError = TestError()
        let core = RecordingCoreAuthorizations()
        let (interactor, _) = await makeInteractor(repository: repository, core: core)

        interactor.revokeOnDisappear(permissions: [.webRtcAccess])
        _ = try await waitUntil { core.resets.count == 1 }

        #expect(core.resets.first?.permissions == [.webRtcAccess])
    }

    @Test
    func revokingNothingTouchesNeither() async throws {
        let repository = RecordingPermissionRepository()
        let core = RecordingCoreAuthorizations()
        let (interactor, _) = await makeInteractor(repository: repository, core: core)

        interactor.revokeOnDisappear(permissions: [])
        try await Task.sleep(for: .milliseconds(100))

        #expect(repository.revoked.isEmpty)
        #expect(core.resets.isEmpty)
    }
}

private struct TestError: Error {}

@MainActor
private func makeInteractor(
    nativeGrants: [ProductPermissionGrant] = [],
    repository: RecordingPermissionRepository = RecordingPermissionRepository(),
    core: RecordingCoreAuthorizations
) -> (AppPermissionsInteractor, GrantsSpy) {
    let interactor = AppPermissionsInteractor(
        productId: "game.paseo",
        providerFactory: SnapshotGrantsProvider(grants: nativeGrants),
        repository: repository,
        coreAuthorizations: core,
        notificationScheduler: IdleNotificationScheduler()
    )
    let presenter = GrantsSpy()
    interactor.presenter = presenter
    return (interactor, presenter)
}

@MainActor
private final class GrantsSpy: AppPermissionsInteractorOutputProtocol {
    private(set) var lastGrants: [ProductPermissionGrant] = []
    private(set) var updates = 0

    func didReceive(grants: [ProductPermissionGrant]) {
        lastGrants = grants
        updates += 1
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

private struct IdleNotificationScheduler: ProductNotificationScheduling {
    func schedule(productId _: ProductId, request _: ScheduledNotificationRequest) async throws -> UInt32 { 0 }
    func cancel(productId _: ProductId, notificationId _: UInt32) async throws {}
    func cancelAll(forProductId _: ProductId) async throws {}
}
