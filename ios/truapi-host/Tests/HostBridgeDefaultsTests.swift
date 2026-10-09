import Foundation
import Testing
import TrUAPIHost

struct HostBridgeDefaultsTests {
    /// A host that runs no workers still hands each operation its own id: the
    /// core counts demand per id, and a host overriding only `endOperation`
    /// would end every operation at once if they all shared one.
    @Test
    func defaultBeginOperationNamesEachOperationSeparately() async throws {
        let bridge = StubHostBridge()

        let first = try await bridge.beginOperation(productId: "test.dot", label: "funding")
        let second = try await bridge.beginOperation(productId: "test.dot", label: "")

        #expect(first != second)
        #expect(first != 0)
        #expect(second != 0)
    }

    @Test
    func localizedTimestampsRespectDSTAndLocalMidnight() async throws {
        let bridge = StubHostBridge()
        let parser = ISO8601DateFormatter()
        let instants = [
            "2024-03-10T06:59:00Z", "2024-03-10T07:01:00Z",
            "2024-03-10T04:59:00Z", "2024-03-10T05:01:00Z",
            "2024-11-03T05:30:00Z", "2024-11-03T06:30:00Z",
        ].map { UInt64(parser.date(from: $0)!.timeIntervalSince1970 * 1_000) }
        let response = try await bridge.localizeTimestamps(request: .init(
            timestampsMs: instants, languageTag: "en-US", timeZone: "America/New_York"
        ))
        #expect(response.timestamps[2].localDate == "2024-03-09")
        #expect(response.timestamps[3].localDate == "2024-03-10")
        let expected = DateFormatter()
        expected.locale = Locale(identifier: "en-US")
        expected.timeZone = TimeZone(identifier: "America/New_York")
        expected.timeStyle = .short
        #expect(response.timestamps[0].time == expected.string(from: parser.date(from: "2024-03-10T06:59:00Z")!))
        #expect(response.timestamps[1].time == expected.string(from: parser.date(from: "2024-03-10T07:01:00Z")!))
        #expect(response.timestamps[4].time == response.timestamps[5].time)
        #expect(response.timestamps[4].dateTime != response.timestamps[5].dateTime)

        let changed = try await bridge.localizeTimestamps(request: .init(
            timestampsMs: instants, languageTag: "fr-FR", timeZone: "Europe/Paris"
        ))
        #expect(changed.timestamps[2].localDate == "2024-03-10")
        #expect(changed.timestamps[0].date != response.timestamps[0].date)
    }

    @Test
    func localizedDateKeysStayGregorianAndUnknownZonesAreRejected() async throws {
        let bridge = StubHostBridge()
        let response = try await bridge.localizeTimestamps(request: .init(
            timestampsMs: [0], languageTag: "th-TH-u-ca-buddhist", timeZone: "Asia/Bangkok"
        ))
        #expect(response.timestamps[0].localDate == "1970-01-01")
        await #expect(throws: HostRejection.self) {
            try await bridge.localizeTimestamps(request: .init(
                timestampsMs: [0], languageTag: "en-US", timeZone: "Not/AZone"
            ))
        }
    }

    @Test
    func processPermissionAdministrationListsImportsAndClosesOnlyRevokedExecutions() async throws {
        let bridge = StubHostBridge()
        let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: HostRuntimeConfig(
            hostName: "permission-tests",
            peopleChainGenesisHash: Data(repeating: 0, count: 32),
            bulletinChainGenesisHash: Data(repeating: 0, count: 32),
            assetHubChainGenesisHash: Data(repeating: 1, count: 32),
            networkSuffix: "paseo",
            databaseDirectory: temporaryDatabaseDirectory()
        ))
        let product = try runtime.openProductExecution(
            bridge: bridge, configuration: .init(productId: "demo.paseo", executionKind: .app)
        )
        let other = try runtime.openProductExecution(
            bridge: bridge, configuration: .init(productId: "other.paseo", executionKind: .app)
        )
        defer {
            product.close()
            other.close()
            runtime.disconnect()
        }
        let request = PermissionAuthorizationRequest.device(.camera)
        try await runtime.setPermissionAuthorizationStatus(productId: "demo.paseo", request: request, status: .authorized)
        #expect(!product.isClosed())
        #expect(try await runtime.permissionAuthorizationProducts().contains("demo.paseo"))
        #expect(try await runtime.permissionAuthorizations(productId: "demo.paseo").contains {
            $0.request == request && $0.status == .authorized
        })
        let revision = try runtime.permissionAuthorizationRevision(productId: "demo.paseo")
        try await runtime.setPermissionAuthorizationStatus(productId: "demo.paseo", request: request, status: .notDetermined)
        #expect(product.isClosed())
        #expect(!other.isClosed())
        // Cross-core storage fanout must still reach the process policy after
        // canonical revocation has closed the originating connection.
        try await product.refreshPermissionAuthorization(request: request)
        #expect(try await runtime.setPermissionAuthorizationStatusIfCurrent(
            productId: "demo.paseo", request: request, status: .authorized, revision: revision
        ) == false)
        _ = try await runtime.importPermissionAuthorizations(
            productId: "demo.paseo", entries: [.init(request: request, status: .authorized)]
        )
        #expect(try await runtime.permissionAuthorizations(productId: "demo.paseo").contains {
            $0.request == request && $0.status == .notDetermined
        })
        #expect(bridge.coreLogs.contains("permissions changed demo.paseo"))
    }

    /// An app that never implemented cards must say so, not pretend it moved one.
    @Test
    func defaultExpandedCardFaceIsUnsupported() async throws {
        let bridge: HostBridge = StubHostBridge()

        let outcome = try await bridge.setExpandedCardFaceShown(shown: false)

        #expect(outcome == .unsupported)
    }
}
