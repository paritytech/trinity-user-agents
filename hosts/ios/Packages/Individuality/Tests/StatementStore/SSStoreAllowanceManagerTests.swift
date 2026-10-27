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

    @Test func ensureFundedAllocatesWithCallerPriority() async throws {
        let capturingRepo = CapturingRepository()
        let allocator = FakeAllowanceSlotAllocating(period: 100)
        let manager = makeManager(
            allocator: allocator,
            repository: AnyDataProviderRepository(capturingRepo),
            slotInfoProvider: FakeSlotInfoProvider()
        )

        try await manager.ensureFunded(accountId: testAccountId, priority: .critical, domain: .chat)

        #expect(allocator.priorities == [.critical])
        #expect(capturingRepo.savedRecords.map(\.accountId) == [testAccountId])
        #expect(capturingRepo.savedRecords.map(\.domain) == [.chat])
    }

    @Test func ensureFundedSkipsAccountWithSlot() async throws {
        let allocator = FakeAllowanceSlotAllocating(period: 100)
        let manager = makeManager(
            allocator: allocator,
            repository: AnyDataProviderRepository(CapturingRepository()),
            slotInfoProvider: CustomFakeSlotInfoProvider(hasExistingSlot: true)
        )

        try await manager.ensureFunded(accountId: testAccountId, priority: .normal, domain: .chat)

        #expect(allocator.assignSlotCalls.isEmpty)
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
            priority: .normal,
            domain: .product
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
            priority: .normal,
            domain: .product
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
            priority: .normal,
            domain: .product
        )

        // Should not call allocator or write to repository
        #expect(capturingRepo.savedRecords.isEmpty)
    }

    @Test(arguments: [(UInt32?.some(100), true), (.some(101), true), (.some(99), false), (nil, false)])
    func hasAllocationWhenRecordCoversPeriod(recordPeriod: UInt32?, expected: Bool) async throws {
        let stored = recordPeriod.map { period in
            AllowanceRecord(
                accountId: testAccountId,
                allocatedAt: Date(),
                kind: .statementStore,
                domain: .chat,
                priority: .critical,
                latestRenewedPeriod: period
            )
        }

        let manager = makeManager(
            allocator: FakeAllowanceSlotAllocating(period: 100),
            repository: AnyDataProviderRepository(CapturingRepository(storedRecords: stored.map { [$0] } ?? [])),
            slotInfoProvider: FakeSlotInfoProvider()
        )

        #expect(try await manager.hasAllocation(accountId: testAccountId, coveringPeriod: 100) == expected)
    }

    @Test func releaseByAccountIdsDeletesWithoutReadingRecords() async throws {
        let capturingRepo = CapturingRepository()

        try await makeManager(
            allocator: FakeAllowanceSlotAllocating(period: 100),
            repository: AnyDataProviderRepository(capturingRepo),
            slotInfoProvider: FakeSlotInfoProvider()
        ).release(with: .accountIds([testAccountId]))

        #expect(capturingRepo.deletedIds == [testAccountId.toHex()])
        #expect(capturingRepo.fetchAllCount == 0)
    }

    @Test func releaseByFilterDeletesSelectedRecords() async throws {
        let otherAccountId = Data(repeating: 0x0F, count: 32)
        let capturingRepo = CapturingRepository(storedRecords: [testAccountId, otherAccountId].map { accountId in
            AllowanceRecord(
                accountId: accountId,
                allocatedAt: Date(),
                kind: .statementStore,
                domain: .chat,
                priority: .critical,
                latestRenewedPeriod: 100
            )
        })

        try await makeManager(
            allocator: FakeAllowanceSlotAllocating(period: 100),
            repository: AnyDataProviderRepository(capturingRepo),
            slotInfoProvider: FakeSlotInfoProvider()
        ).release(with: .filter(SelectingReleaseFilter(accountIds: [testAccountId])))

        #expect(capturingRepo.deletedIds == [testAccountId.toHex()])
        #expect(capturingRepo.savedRecords.isEmpty)
    }
}

private struct SelectingReleaseFilter: AllowanceReleaseFiltering {
    let accountIds: Set<AccountId>

    func recordsToRelease(from records: [AllowanceRecord]) async throws -> [AllowanceRecord] {
        records.filter { accountIds.contains($0.accountId) }
    }
}

// MARK: - CapturingRepository

final class CapturingRepository: DataProviderRepositoryProtocol {
    typealias Model = AllowanceRecord

    private let storedRecords: [AllowanceRecord]
    private(set) var savedRecords: [AllowanceRecord] = []
    private(set) var deletedIds: [String] = []
    private(set) var fetchAllCount = 0

    init(storedRecords: [AllowanceRecord] = []) {
        self.storedRecords = storedRecords
    }

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
        by identifierBlock: @escaping () throws -> String,
        options _: RepositoryFetchOptions
    ) -> BaseOperation<Model?> {
        ClosureOperation { [storedRecords] in
            let identifier = try identifierBlock()
            return storedRecords.first { $0.identifier == identifier }
        }
    }

    func fetchAllOperation(with _: RepositoryFetchOptions) -> BaseOperation<[Model]> {
        fetchAllCount += 1
        return BaseOperation.createWithResult(storedRecords)
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

    func freeSlot(
        excluding _: AccountId,
        callerPriority _: AllowanceRecord.Priority
    ) async throws -> SSSSlotInfo {
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
