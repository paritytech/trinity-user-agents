import Foundation
import Testing
@testable import TrUAPIHost
import UIKit

struct TrUAPIWsBridgeTests {
    @Test(.timeLimit(.minutes(1)))
    func testFeatureSupportedRoundTripOverWsBridge() async throws {
        let bridge = StubHostBridge()
        let runtime = try TrUAPIHostRuntime(
            bridge: bridge,
            runtimeConfig: Self.makeHostRuntimeConfig()
        )
        let execution = try runtime.openProductExecution(
            bridge: bridge,
            configuration: ProductExecutionConfig(
                productId: "test.dot",
                executionKind: .app
            )
        )

        let endpoint = try execution.startWsBridge(bindPort: 0)
        defer { execution.stopWsBridge() }

        let url = try #require(URL(string: "ws://127.0.0.1:\(endpoint.port)/?t=\(endpoint.token)"))
        let task = URLSession.shared.webSocketTask(with: url)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        try await task.send(.data(Self.featureSupportedRequestFrame()))
        let message = try await task.receive()

        guard case let .data(response) = message else {
            Issue.record("expected binary frame, got \(message)")
            return
        }
        // Frame tail: message_type=Response(0x01), then the payload's own
        // bytes, Result::Ok(0x00), V1(0x00), supported(0x01).
        #expect(response.suffix(4) == Data([0x01, 0x00, 0x00, 0x01]))
    }

    /// An iOS host must classify itself as `Ios` without the embedding app
    /// saying so, since the only way to reach this package is from an iOS app.
    @Test(.timeLimit(.minutes(1)))
    func testHostInfoReportsTheIosPlatform() async throws {
        let bridge = StubHostBridge()
        let runtime = try TrUAPIHostRuntime(
            bridge: bridge,
            runtimeConfig: Self.makeHostRuntimeConfig()
        )
        let execution = try runtime.openProductExecution(
            bridge: bridge,
            configuration: ProductExecutionConfig(
                productId: "test.dot",
                executionKind: .app
            )
        )

        let endpoint = try execution.startWsBridge(bindPort: 0)
        defer { execution.stopWsBridge() }

        let url = try #require(URL(string: "ws://127.0.0.1:\(endpoint.port)/?t=\(endpoint.token)"))
        let task = URLSession.shared.webSocketTask(with: url)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        try await task.send(.data(Self.hostInfoRequestFrame()))
        let message = try await task.receive()

        guard case let .data(response) = message else {
            Issue.record("expected binary frame, got \(message)")
            return
        }
        #expect(response.suffix(Self.hostInfoResponseTail.count) == Self.hostInfoResponseTail)
    }

    /// iOS reclaims a suspended app's listening socket while products keep
    /// the endpoint they were given, so returning to the foreground must
    /// rebind the listener on that same port.
    @Test(.timeLimit(.minutes(1)))
    func testReturningToTheForegroundRebindsTheBridgeOnItsPort() async throws {
        let bridge = StubHostBridge()
        let notifications = NotificationCenter()
        let runtime = try TrUAPIHostRuntime(
            bridge: bridge,
            runtimeConfig: Self.makeHostRuntimeConfig(),
            notificationCenter: notifications
        )
        let execution = try runtime.openProductExecution(
            bridge: bridge,
            configuration: ProductExecutionConfig(
                productId: "test.dot",
                executionKind: .app
            )
        )
        let endpoint = try execution.startWsBridge(bindPort: 0)
        defer { execution.stopWsBridge() }

        withExtendedLifetime(runtime) {
            notifications.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        }

        let rebound = "truapi.ws_bridge.relistened port=\(endpoint.port)"
        let deadline = Date().addingTimeInterval(5)
        while !bridge.coreLogs.contains(rebound), Date() < deadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(bridge.coreLogs.contains(rebound))
        let url = try #require(URL(string: "ws://127.0.0.1:\(endpoint.port)/?t=\(endpoint.token)"))
        let task = URLSession.shared.webSocketTask(with: url)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        try await task.send(.data(Self.featureSupportedRequestFrame()))
        let message = try await task.receive()

        guard case let .data(response) = message else {
            Issue.record("expected binary frame, got \(message)")
            return
        }
        #expect(response.suffix(4) == Data([0x01, 0x00, 0x00, 0x01]))
    }
}

private extension TrUAPIWsBridgeTests {
    static func makeHostRuntimeConfig() throws -> HostRuntimeConfig {
        try HostRuntimeConfig(
            hostName: "truapi-host-tests",
            peopleChainGenesisHash: Data(repeating: 0, count: 32),
            bulletinChainGenesisHash: Data(repeating: 0, count: 32),
            // Non-zero: all-zero is the "no Asset Hub" sentinel, and this
            // fixture is not exercising that case.
            assetHubChainGenesisHash: Data(repeating: 1, count: 32),
            networkSuffix: "paseo",
            databaseDirectory: temporaryDatabaseDirectory()
        )
    }

    // wire_table.rs: SYSTEM_FEATURE_SUPPORTED { trait_id: 1, method_id: 1 }.
    // Both bytes are load-bearing: a lone method byte is read as the trait and
    // routes into a different trait's method 0 rather than failing.
    static let featureSupportedDiscriminant = Data([0x01, 0x01])

    // wire_table.rs: SYSTEM_HOST_INFO { trait_id: 1, method_id: 3 }.
    static let hostInfoDiscriminant = Data([0x01, 0x03])

    static func hostInfoRequestFrame() -> Data {
        var frame = Data()
        frame.append(contentsOf: [0x0C]) // compact length 3
        frame.append("p:1".data(using: .utf8)!)
        frame.append(hostInfoDiscriminant) // from wire_table.rs
        // message_type=Request(0x00), then the payload: V1(0x00).
        frame.append(contentsOf: [0x00, 0x00])
        return frame
    }

    // Response tail: message_type=Response(0x01), then the payload,
    // Result::Ok(0x00), V1(0x00), then HostInfo as platform(Ios = 0x02),
    // name, version (empty, hostVersion is unset).
    static var hostInfoResponseTail: Data {
        var tail = Data([0x01, 0x00, 0x00, 0x02, 0x44]) // 0x44 is compact length 17
        tail.append("truapi-host-tests".data(using: .utf8)!)
        tail.append(contentsOf: [0x00])
        return tail
    }

    static func featureSupportedRequestFrame() -> Data {
        var frame = Data()
        frame.append(contentsOf: [0x0C]) // compact length 3
        frame.append("p:1".data(using: .utf8)!)
        frame.append(featureSupportedDiscriminant) // from wire_table.rs
        // message_type=Request(0x00), then the payload: V1(0x00), Chain(0x00),
        // compact(32) (0x80).
        frame.append(contentsOf: [0x00, 0x00, 0x00, 0x80])
        frame.append(Data(repeating: 0, count: 32))
        return frame
    }
}

final class StubStorage: HostStorageBackend, @unchecked Sendable {
    private var store: [String: Data] = [:]

    func read(key: String) throws -> Data? { store[key] }
    func write(key: String, value: Data) throws { store[key] = value }
    func clear(key: String) throws { store[key] = nil }
}

final class StubCoreStorage: HostCoreStorageBackend, @unchecked Sendable {
    private let lock = NSLock()
    private var store: [Data: Data] = [:]

    func read(key: Data) throws -> Data? {
        lock.withLock { store[key] }
    }

    func write(key: Data, value: Data) throws {
        lock.withLock { store[key] = value }
    }

    func clear(key: Data) throws {
        lock.withLock { store[key] = nil }
    }
}

// Conforms to HostBridge rather than the generated HostCallbacks, so the
// protocol extension supplies every optional callback and a new one cannot
// leave this file behind. Only the six requirements without a default are
// written out, plus the core log recorder.
/// A fresh directory for one runtime's core database.
func temporaryDatabaseDirectory() throws -> String {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory.path
}

final class StubHostBridge: HostBridge, @unchecked Sendable {
    let storage: HostStorageBackend = StubStorage()
    let coreStorage: HostCoreStorageBackend = StubCoreStorage()
    private let logLock = NSLock()
    private var logs: [String] = []
    private let permissionLock = NSLock()
    private var remoteDecisions: [PermissionDecision]
    private var deviceDecisions: [PermissionDecision]
    private var deviceRequests: [HostDevicePermissionRequest] = []

    init(remoteDecisions: [PermissionDecision] = [], deviceDecisions: [PermissionDecision] = []) {
        self.remoteDecisions = remoteDecisions
        self.deviceDecisions = deviceDecisions
    }

    var requestedDevicePermissions: [HostDevicePermissionRequest] {
        permissionLock.withLock { deviceRequests }
    }

    var coreLogs: [String] {
        logLock.withLock { logs }
    }

    func onCoreLog(marker: String, detail: String) {
        logLock.withLock { logs.append("\(marker) \(detail)") }
    }

    private func nextDeviceDecision(request: HostDevicePermissionRequest) -> PermissionDecision {
        permissionLock.withLock {
            deviceRequests.append(request)
            return deviceDecisions.isEmpty ? .deny : deviceDecisions.removeFirst()
        }
    }

    private func nextRemoteDecision() -> PermissionDecision {
        permissionLock.withLock {
            remoteDecisions.isEmpty ? .deny : remoteDecisions.removeFirst()
        }
    }

    func navigateTo(url _: String) async throws {}
    func devicePermission(
        product _: ProductExecutionConfig,
        request: HostDevicePermissionRequest
    ) async throws -> PermissionDecision {
        nextDeviceDecision(request: request)
    }
    func remotePermission(
        product _: ProductExecutionConfig,
        request _: RemotePermission
    ) async throws -> PermissionDecision { nextRemoteDecision() }
    func featureSupported(request _: HostFeatureSupportedRequest) async throws -> Bool { true }
    func supportedChains() throws -> HostChainSet { HostChainSet(network: "", chains: []) }
    func localStorageRead(key: String) throws -> Data? { try storage.read(key: key) }
    
    func localStorageWrite(key: String, value: Data) throws {
        try storage.write(key: key, value: value)
    }
    
    func localStorageClear(key: String) throws { try storage.clear(key: key) }
}

// Conforms to `ChatHostBridge` so a new requirement there fails this job.
// Every member is written out: the protocol supplies no defaults.
final class StubChatHostBridge: ChatHostBridge {
    func createRoom(
        roomId _: String,
        name _: String,
        icon _: String
    ) async throws -> ChatRoomRegistrationStatus { .new }

    func registerBot(
        botId _: String,
        name _: String,
        icon _: String
    ) async throws -> ChatBotRegistrationStatus { .new }

    func postMessage(roomId _: String, content _: ChatMessageContent) async throws -> String {
        "message-id"
    }

    func listRooms() async throws -> [ChatRoom] { [] }
}

// Conforms to `PocketHostBridge` so a new requirement there fails this job.
// Every member is written out: the protocol supplies no defaults.
final class StubPocketHostBridge: PocketHostBridge {
    func listCards() throws -> [PocketCard] { [] }

    func removeCard(cardId _: String) throws -> NativePocketRemoval { .absent }
}
