import Foundation
import SubstrateSdk
import IssueMonitoring
import Testing
import Operation_iOS
import BackgroundExecution
@testable import Individuality

struct SSStoreAllowanceManagerTests {
    private let keyManager = FakeBandersnatchKeyManaging()
    private let testAccountId = Data([0x01])
    private let testCollection = "test-collection"

    private func makePersonOrigin() -> PersonOrigin {
        .lite(0, keyManager)
    }

    private func makeManager(
        allocator: AllowanceSlotAllocating,
        repository: AnyDataProviderRepository<AllowanceRecord>,
        slotInfoProvider: StatementStoreSlotInfoProviding,
        renewer: StatementStoreSlotRenewing = FakeStatementStoreSlotRenewer()
    ) -> SSStoreAllowanceManager {
        SSStoreAllowanceManager(
            repository: repository,
            allocator: allocator,
            slotInfoProvider: slotInfoProvider,
            renewer: renewer,
            backgroundExecutor: InlineBackgroundExecutor(),
            issueDiagnostics: .make(flow: "statement-store-allowance", reporter: NoopIssueReporter())
        )
    }

    // Test: allocate without eviction
    @Test func allocateUpsertsRecord() async throws {
        let capturingRepo = CapturingRepository()
        let repository = AnyDataProviderRepository(capturingRepo)
        let slotProvider = FakeSlotInfoProvider()

        let allocator = FakeAllowanceSlotAllocating(
            period: 100
        )

        let manager = makeManager(
            allocator: allocator,
            repository: repository,
            slotInfoProvider: slotProvider
        )

        try await manager.allocate(
            accountId: testAccountId,
            policy: .ignore,
            priority: .normal
        )

        #expect(capturingRepo.savedRecords.count == 1)
        #expect(capturingRepo.savedRecords[0].accountId == testAccountId)
        #expect(capturingRepo.savedRecords[0].latestRenewedPeriod == 100)
    }

    // Test: allocate never triggers eviction cleanup
    @Test func allocateNeverCallsDeleteFor() async throws {
        let capturingRepo = CapturingRepository()
        let repository = AnyDataProviderRepository(capturingRepo)
        let slotProvider = FakeSlotInfoProvider()

        let allocator = FakeAllowanceSlotAllocating(
            period: 100
        )

        let manager = makeManager(
            allocator: allocator,
            repository: repository,
            slotInfoProvider: slotProvider
        )

        try await manager.allocate(
            accountId: testAccountId,
            policy: .increase,
            priority: .normal
        )

        #expect(capturingRepo.savedRecords.count == 1)
        #expect(capturingRepo.savedRecords[0].accountId == testAccountId)
    }

    // Test: allocate with .ignore policy and existing slot
    @Test func allocateWithIgnorePolicyAndExistingSlotSkipsAllocation() async throws {
        let capturingRepo = CapturingRepository()
        let repository = AnyDataProviderRepository(capturingRepo)

        // Set up provider to return true for hasExistingSlot
        let slotProvider = CustomFakeSlotInfoProvider(hasExistingSlot: true)

        let allocator = FakeAllowanceSlotAllocating(
            period: 100
        )

        let manager = makeManager(
            allocator: allocator,
            repository: repository,
            slotInfoProvider: slotProvider
        )

        try await manager.allocate(
            accountId: testAccountId,
            policy: .ignore,
            priority: .normal
        )

        // Should not call allocator or write to repository
        #expect(capturingRepo.savedRecords.isEmpty)
    }

    @Test func releaseDeletesRecordByAccountId() async throws {
        let capturingRepo = CapturingRepository()
        let repository = AnyDataProviderRepository(capturingRepo)

        let manager = makeManager(
            allocator: FakeAllowanceSlotAllocating(period: 100),
            repository: repository,
            slotInfoProvider: FakeSlotInfoProvider()
        )

        try await manager.release(accountId: testAccountId)

        #expect(capturingRepo.deletedIds == [testAccountId.toHex()])
        #expect(capturingRepo.savedRecords.isEmpty)
    }
}

// MARK: - CapturingRepository

final class CapturingRepository: DataProviderRepositoryProtocol {
    typealias Model = AllowanceRecord

    private(set) var savedRecords: [AllowanceRecord] = []
    private(set) var deletedIds: [String] = []

    func saveOperation(
        _ updateModelsBlock: @escaping () throws -> [Model],
        _ deleteIdsBlock: @escaping () throws -> [String]
    ) -> BaseOperation<Void> {
        let operation = ClosureOperation { [weak self] in
            let records = try updateModelsBlock()
            let identifiers = try deleteIdsBlock()
            self?.savedRecords.append(contentsOf: records)
            self?.deletedIds.append(contentsOf: identifiers)
        }
        return operation
    }

    func fetchOperation(
        by _: @escaping () throws -> String,
        options _: RepositoryFetchOptions
    ) -> BaseOperation<Model?> {
        fatalError("Should not be called in tests")
    }

    func fetchAllOperation(with _: RepositoryFetchOptions) -> BaseOperation<[Model]> {
        fatalError("Should not be called in tests")
    }

    func fetchOperation(
        by _: RepositorySliceRequest,
        options _: RepositoryFetchOptions
    ) -> BaseOperation<[Model]> {
        fatalError("Should not be called in tests")
    }

    func replaceOperation(_ _: @escaping () throws -> [Model]) -> BaseOperation<Void> {
        fatalError("Should not be called in tests")
    }

    func fetchCountOperation() -> BaseOperation<Int> {
        fatalError("Should not be called in tests")
    }

    func deleteAllOperation() -> BaseOperation<Void> {
        fatalError("Should not be called in tests")
    }
}

// MARK: - CustomFakeSlotInfoProvider

final class CustomFakeSlotInfoProvider: StatementStoreSlotInfoProviding {
    private let existingSlot: Bool

    init(hasExistingSlot: Bool) {
        existingSlot = hasExistingSlot
    }

    func hasExistingSlot(for _: AccountId) async throws -> Bool {
        existingSlot
    }

    func freeSlot(excluding _: AccountId, callerPriority _: AllowanceRecord.Priority) async throws -> SSSSlotInfo {
        fatalError("Should not be called")
    }

    func renewalSlots(period _: UInt32) async throws -> [SSSRenewalSlot] {
        []
    }
}

// MARK: - FakeStatementStoreSlotRenewer

final class FakeStatementStoreSlotRenewer: StatementStoreSlotRenewing {
    func renew() async throws {}
}
