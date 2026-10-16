import os
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIPermissionResetterTests {
    @Test
    func clearsEachPermissionOnOneConnectionAndClosesIt() throws {
        let execution = MockProductExecution()
        let opened = OSAllocatedUnfairLock(initialState: [ProductId]())
        let resetter = TrUAPIPermissionResetter { productId in
            opened.withLock { $0.append(productId) }
            return execution
        }

        try resetter.reset(
            productId: "game.paseo",
            permissions: [.networkAccess(domain: "api.example.com"), .deviceCapability(.camera)]
        )

        #expect(opened.withLock { $0 } == ["game.paseo"])
        #expect(execution.permissionStatusUpdates.map(\.0) == [
            .remote(RemotePermissionRequest(permission: .remote(domains: ["api.example.com"]))),
            .device(.camera)
        ])
        #expect(execution.permissionStatusUpdates.allSatisfy { $0.1 == .notDetermined })
        #expect(execution.closeCallCount == 1)
    }

    @Test
    func opensNoConnectionWhenThereIsNothingToClear() throws {
        let opened = OSAllocatedUnfairLock(initialState: 0)
        let resetter = TrUAPIPermissionResetter { _ in
            opened.withLock { $0 += 1 }
            return MockProductExecution()
        }

        try resetter.reset(productId: "game.paseo", permissions: [])

        #expect(opened.withLock { $0 } == 0)
    }
}
