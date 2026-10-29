import Foundation
import os
import AsyncExtensions
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIPermissionAuthorizationsTests {
    @Test
    func listsOnlyStandingGrantsAsTheSettingsPermissions() async throws {
        let runtime = RecordingRuntime()
        runtime.authorizations = [
            authorization("game.paseo", .device(.camera), .authorized),
            authorization(
                "game.paseo",
                .remote(.init(permission: .remote(domains: ["a.example", "b.example"]))),
                .authorized
            ),
            authorization("game.paseo", .remote(.init(permission: .balanceAccess)), .denied),
            authorization("stash.paseo", .identityDisclosure, .authorized),
            authorization(
                "stash.paseo",
                .crossProductSignature(targetProductId: "game.paseo", kind: .payload),
                .authorized
            )
        ]
        let authorizations = makeAuthorizations(runtime: runtime)

        #expect(try await authorizations.grants() == [
            grant("game.paseo", .deviceCapability(.camera)),
            grant("game.paseo", .networkAccess(domain: "a.example")),
            grant("game.paseo", .networkAccess(domain: "b.example")),
            grant("stash.paseo", .userIdentityAccess)
        ])
    }

    @Test
    func resetClearsEachPermissionInTheCore() throws {
        let runtime = RecordingRuntime()
        let authorizations = makeAuthorizations(runtime: runtime)

        try authorizations.reset(
            productId: "game.paseo",
            permissions: [.networkAccess(domain: "api.example.com"), .deviceCapability(.camera)]
        )

        #expect(runtime.updates == [
            .init(productId: "game.paseo", request: .remote(.init(permission: .remote(domains: ["api.example.com"])))),
            .init(productId: "game.paseo", request: .device(.camera))
        ])
    }

    @Test
    func resettingNothingLeavesTheRuntimeUntouched() throws {
        let opened = OSAllocatedUnfairLock(initialState: 0)
        let authorizations = TrUAPIPermissionAuthorizations(
            runtime: {
                opened.withLock { $0 += 1 }
                return RecordingRuntime()
            },
            changes: AsyncPassthroughSubject<ProductId>().eraseToAnyAsyncSequence()
        )

        try authorizations.reset(productId: "game.paseo", permissions: [])

        #expect(opened.withLock { $0 } == 0)
    }
}

private func makeAuthorizations(runtime: RecordingRuntime) -> TrUAPIPermissionAuthorizations {
    TrUAPIPermissionAuthorizations(
        runtime: { runtime },
        changes: AsyncPassthroughSubject<ProductId>().eraseToAnyAsyncSequence()
    )
}

private func authorization(
    _ productId: ProductId,
    _ request: PermissionAuthorizationRequest,
    _ status: PermissionAuthorizationStatus
) -> ProductPermissionAuthorization {
    ProductPermissionAuthorization(productId: productId, request: request, status: status)
}

private final class RecordingRuntime: TrUAPIPermissionAdministering, @unchecked Sendable {
    struct Update: Equatable {
        let productId: String
        let request: PermissionAuthorizationRequest
    }

    private let state = OSAllocatedUnfairLock(initialState: [Update]())
    var authorizations: [ProductPermissionAuthorization] = []

    var updates: [Update] {
        state.withLock { $0 }
    }

    func permissionAuthorizations() async throws -> [ProductPermissionAuthorization] {
        authorizations
    }

    func setPermissionAuthorizationStatus(
        productId: String,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus
    ) throws {
        #expect(status == .notDetermined)
        state.withLock { $0.append(Update(productId: productId, request: request)) }
    }
}
