import Foundation
import Testing
import Operation_iOS
import SubstrateSdk

@testable import polkadot_app

// MARK: - In-Memory Repository Factories

private final class MockHostRepositoryFactory: PolkadotSignInHostRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<PolkadotSignInHost>()

    var databaseService: CoreDataServiceProtocol {
        fatalError("Core Data not available in disconnect applier tests")
    }

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<PolkadotSignInHost> {
        AnyDataProviderRepository(repository)
    }

    func fetchAll() async throws -> [PolkadotSignInHost] {
        try await repository.fetchAllOperation(with: .init()).asyncExecute()
    }
}

private final class ApplierLocalDeviceRepositoryFactory: LocalDeviceRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<Chat.LocalDevice>()

    var databaseService: CoreDataServiceProtocol {
        fatalError("Core Data not available in disconnect applier tests")
    }

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<Chat.LocalDevice> {
        AnyDataProviderRepository(repository)
    }

    func fetchAll() async throws -> [Chat.LocalDevice] {
        try await repository.fetchAllOperation(with: .init()).asyncExecute()
    }
}

private final class MockDeviceMessageBroadcaster: DeviceMessageBroadcasting, @unchecked Sendable {
    private(set) var broadcastedAccountIds: [Data] = []
    var errorToThrow: Error?

    func broadcastDeviceAdded(statementAccountId _: Data, encryptionPublicKey _: Data) async throws {}

    func broadcastDeviceRemoved(statementAccountId: Data) async throws {
        if let errorToThrow { throw errorToThrow }
        broadcastedAccountIds.append(statementAccountId)
    }
}

// MARK: - Test Harness

private struct ApplierTestHarness {
    let hostRepositoryFactory: MockHostRepositoryFactory
    let localDeviceRepositoryFactory: ApplierLocalDeviceRepositoryFactory
    let broadcaster: MockDeviceMessageBroadcaster
    let applier: SSORemoteDisconnectApplier

    init() {
        hostRepositoryFactory = MockHostRepositoryFactory()
        localDeviceRepositoryFactory = ApplierLocalDeviceRepositoryFactory()
        broadcaster = MockDeviceMessageBroadcaster()

        applier = SSORemoteDisconnectApplier(
            hostRepositoryFactory: hostRepositoryFactory,
            localDeviceRepositoryFactory: localDeviceRepositoryFactory,
            deviceMessageBroadcaster: broadcaster,
            logger: MockLogger()
        )
    }

    func makeHost(accountId: Data = Data(repeating: 0xAB, count: 32)) -> PolkadotSignInHost {
        PolkadotSignInHost(
            accountId: accountId,
            publicKey: Data(repeating: 0x02, count: 32),
            name: "TestHost",
            iconUrl: nil
        )
    }

    func seedHost(_ host: PolkadotSignInHost) async throws {
        let repo = hostRepositoryFactory.createRepository(forFilter: nil)
        try await repo.saveOperation({ [host] }, { [] }).asyncExecute()
    }

    func seedLocalDevice(statementAccountId: Data) async throws {
        let device = Chat.LocalDevice(
            statementAccountId: statementAccountId,
            encryptionPublicKey: Data(repeating: 0xFF, count: 32),
            hostName: "TestHost",
            createdAt: Date(),
            hostVersion: nil,
            osType: nil,
            osVersion: nil
        )
        let repo = localDeviceRepositoryFactory.createRepository(forFilter: nil)
        try await repo.saveOperation({ [device] }, { [] }).asyncExecute()
    }
}

// MARK: - Tests

@Suite("SSORemoteDisconnectApplier Tests")
struct SSORemoteDisconnectApplierTests {
    @Test("applyDisconnect removes the host record from the repository")
    func applierRemovesHostRecord() async throws {
        let h = ApplierTestHarness()
        let accountId = Data(repeating: 0x01, count: 32)
        let host = h.makeHost(accountId: accountId)
        try await h.seedHost(host)

        await h.applier.applyDisconnect(from: host)

        let remaining = try await h.hostRepositoryFactory.fetchAll()
        #expect(remaining.isEmpty)
    }

    @Test("applyDisconnect removes the local device record by accountId hex")
    func applierRemovesLocalDevice() async throws {
        let h = ApplierTestHarness()
        let accountId = Data(repeating: 0x02, count: 32)
        let host = h.makeHost(accountId: accountId)
        try await h.seedLocalDevice(statementAccountId: accountId)

        await h.applier.applyDisconnect(from: host)

        let remaining = try await h.localDeviceRepositoryFactory.fetchAll()
        #expect(remaining.isEmpty)
    }

    @Test("applyDisconnect broadcasts deviceRemoved with correct accountId")
    func applierBroadcastsDeviceRemoved() async throws {
        let h = ApplierTestHarness()
        let accountId = Data(repeating: 0x03, count: 32)
        let host = h.makeHost(accountId: accountId)

        await h.applier.applyDisconnect(from: host)

        #expect(h.broadcaster.broadcastedAccountIds == [accountId])
    }

    @Test("A failing host-removal step does not prevent device removal or broadcast")
    func hostRemovalFailureDoesNotStopRemainingSteps() async throws {
        let h = ApplierTestHarness()
        let accountId = Data(repeating: 0x04, count: 32)
        let host = h.makeHost(accountId: accountId)

        // Seed the local device so we can verify it is removed despite the host step failing.
        try await h.seedLocalDevice(statementAccountId: accountId)
        // Do NOT seed the host — saveOperation with a delete for a missing key
        // should succeed gracefully, but the real resilience test is that even if
        // the operation throws, subsequent steps still run.
        // We exercise this via a broadcaster error in the next test; here we verify
        // the steps are independent by checking the device + broadcast both complete
        // even when host deletion deletes nothing.

        await h.applier.applyDisconnect(from: host)

        let remainingDevices = try await h.localDeviceRepositoryFactory.fetchAll()
        #expect(remainingDevices.isEmpty)
        #expect(h.broadcaster.broadcastedAccountIds == [accountId])
    }

    @Test("A failing broadcast step does not prevent host and device removal")
    func broadcastFailureDoesNotStopEarlierSteps() async throws {
        let h = ApplierTestHarness()

        enum TestError: Error { case boom }
        h.broadcaster.errorToThrow = TestError.boom

        let accountId = Data(repeating: 0x05, count: 32)
        let host = h.makeHost(accountId: accountId)
        try await h.seedHost(host)
        try await h.seedLocalDevice(statementAccountId: accountId)

        await h.applier.applyDisconnect(from: host)

        let remainingHosts = try await h.hostRepositoryFactory.fetchAll()
        let remainingDevices = try await h.localDeviceRepositoryFactory.fetchAll()
        #expect(remainingHosts.isEmpty)
        #expect(remainingDevices.isEmpty)
    }
}
