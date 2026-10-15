import Foundation
import SubstrateSdk
import SDKLogger
import KeyDerivation
import ExtrinsicService
import Operation_iOS
import BandersnatchApi
import BackgroundExecution
import Individuality

// MARK: - FakeSlotAccounting

final class FakeSlotAccounting: StatementStoreSlotAccounting {
    private var records: [AllowanceRecord] = []
    private(set) var deleteForCalls: [AccountId] = []
    private(set) var markRenewedCalls: [
        (record: AllowanceRecord, period: UInt32, since: Date)
    ] = []

    func setInitialRecords(_ records: [AllowanceRecord]) {
        self.records = records
    }

    func deleteFor(accountId: AccountId) async throws {
        deleteForCalls.append(accountId)
        records.removeAll { $0.accountId == accountId }
    }

    func staleRows(currentPeriod: UInt32) async throws -> [AllowanceRecord] {
        records.filter { record in
            record.latestRenewedPeriod != nil && record.latestRenewedPeriod! < currentPeriod
        }
    }

    func markRenewed(_ record: AllowanceRecord, period: UInt32, since: Date) async throws {
        let updated = AllowanceRecord(
            accountId: record.accountId,
            allocatedAt: since,
            kind: record.kind,
            priority: record.priority,
            latestRenewedPeriod: period
        )
        markRenewedCalls.append((record: updated, period: period, since: since))
        records.removeAll { $0.accountId == record.accountId }
        records.append(updated)
    }

    func priorities(for accountIds: [AccountId]) async throws -> [AccountId: AllowanceRecord.Priority] {
        let requestedIds = Set(accountIds)
        let filtered = records.filter { requestedIds.contains($0.accountId) }
        var result: [AccountId: AllowanceRecord.Priority] = [:]
        for record in filtered {
            result[record.accountId] = record.priority
        }
        return result
    }
}

// MARK: - FakeBandersnatchKeyManaging

final class FakeBandersnatchKeyManaging: BandersnatchKeyManaging {
    func getRawPublicKey() throws -> Data {
        Data(repeating: 0x01, count: 32)
    }

    func sign(_: Data) throws -> Data {
        Data(repeating: 0x02, count: 64)
    }

    func createProof(
        _: Data,
        members _: [Data],
        context _: Data,
        domainSize _: BandersnatchApi.RingDomainSize
    ) throws -> Data {
        Data(repeating: 0x03, count: 128)
    }

    func deriveAlias(for _: Data) throws -> Data {
        Data(repeating: 0x01, count: 32)
    }
}

// MARK: - FakeSlotInfoProvider

struct FakeSlotInfoProviderConfig {
    var renewalSlots: Result<[SSSRenewalSlot], Error> = .success([])
    var freeSlot: Result<SSSSlotInfo, Error> = .failure(AllowanceSlotAssignmentError.noSlotsAvailable)
}

final class FakeSlotInfoProvider: StatementStoreSlotInfoProviding {
    private var config: FakeSlotInfoProviderConfig

    init(config: FakeSlotInfoProviderConfig = FakeSlotInfoProviderConfig()) {
        self.config = config
    }

    func updateRenewalSlots(_ result: Result<[SSSRenewalSlot], Error>) {
        config.renewalSlots = result
    }

    func updateFreeSlot(_ result: Result<SSSSlotInfo, Error>) {
        config.freeSlot = result
    }

    func hasExistingSlot(for _: AccountId) async throws -> Bool {
        false
    }

    func freeSlot(excluding _: AccountId, callerPriority _: AllowanceRecord.Priority) async throws -> SSSSlotInfo {
        switch config.freeSlot {
        case let .success(slot):
            return slot
        case let .failure(error):
            throw error
        }
    }

    func renewalSlots(period _: UInt32) async throws -> [SSSRenewalSlot] {
        switch config.renewalSlots {
        case let .success(slots):
            return slots
        case let .failure(error):
            throw error
        }
    }
}

// MARK: - FakeSubmitter

final class FakeSubmitter: SlotAssignmentSubmitting {
    private(set) var submitCalls: [ChainId] = []
    var failureSequence: [Bool] = []
    var failureError: Error?
    private var failureIndex: Int = 0

    func submit(
        call _: any RuntimeCallable,
        makeOrigin: @Sendable (ChainId) async throws -> ExtrinsicOriginDefining,
        chainId: ChainId
    ) async throws {
        _ = try await makeOrigin(chainId)
        submitCalls.append(chainId)

        if failureIndex < failureSequence.count, failureSequence[failureIndex] {
            failureIndex += 1
            throw failureError ?? NSError(domain: "FakeSubmitter", code: -1, userInfo: nil)
        }
        failureIndex += 1
    }
}

// MARK: - FakeChainTimeProvider

final class FakeChainTimeProvider: ChainTimeProviding {
    var scriptedSeconds: [UInt64] = []
    private(set) var nowSecondsValue: UInt64

    init(period: UInt32 = 100) {
        nowSecondsValue = UInt64(period) * UInt64(TimeInterval.secondsInDay)
    }

    func setPeriod(_ period: UInt32) {
        nowSecondsValue = UInt64(period) * UInt64(TimeInterval.secondsInDay)
    }

    func nowSeconds(at _: Data?) async throws -> UInt64 {
        if !scriptedSeconds.isEmpty { return scriptedSeconds.removeFirst() }
        return nowSecondsValue
    }
}

// MARK: - StubExtrinsicOrigin

struct StubExtrinsicOrigin: ExtrinsicOriginDefining {
    func createOriginResolutionWrapper(
        for _: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion _: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        fatalError("Stub: createOriginResolutionWrapper should never be called in tests")
    }
}

// MARK: - FakeOriginFactory

final class FakeOriginFactory: AsResourcesOriginCreating {
    private(set) var sssOriginCalls: [(personOrigin: PersonOrigin, period: UInt32, seq: UInt32)] = []

    func createSSSOrigin(
        personOrigin: PersonOrigin,
        period: UInt32,
        seq: UInt32,
        chain _: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        sssOriginCalls.append((personOrigin: personOrigin, period: period, seq: seq))
        return StubExtrinsicOrigin()
    }

    func createLTSOrigin(
        personOrigin _: PersonOrigin,
        period _: UInt32,
        counter _: UInt8,
        chain _: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        StubExtrinsicOrigin()
    }

    private(set) var notificationOriginCalls: [(personOrigin: PersonOrigin, period: UInt32, seq: UInt8)] = []

    func createNotificationOrigin(
        personOrigin: PersonOrigin,
        period: UInt32,
        seq: UInt8,
        chain _: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        notificationOriginCalls.append((personOrigin: personOrigin, period: period, seq: seq))
        return StubExtrinsicOrigin()
    }
}

// MARK: - FakeLogger

final class FakeLogger: SDKLoggerProtocol {
    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {
        // No-op
    }

    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {
        // No-op
    }

    func info(message _: () -> String, file _: String, function _: String, line _: Int) {
        // No-op
    }

    func warning(message _: () -> String, file _: String, function _: String, line _: Int) {
        // No-op
    }

    func error(message _: () -> String, file _: String, function _: String, line _: Int) {
        // No-op
    }
}

// MARK: - FakeAllowanceSlotAllocating

final class FakeAllowanceSlotAllocating: AllowanceSlotAllocating {
    private let period: UInt32
    private(set) var assignSlotCalls: [AccountId] = []

    init(period: UInt32) {
        self.period = period
    }

    func assignSlot(accountId: AccountId, priority _: AllowanceRecord.Priority) async throws -> UInt32 {
        assignSlotCalls.append(accountId)
        return period
    }
}

/// Runs the operation inline, no OS background assertion — deterministic for tests.
struct InlineBackgroundExecutor: BackgroundExecuting {
    func execute<T: Sendable>(
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await operation()
    }
}
