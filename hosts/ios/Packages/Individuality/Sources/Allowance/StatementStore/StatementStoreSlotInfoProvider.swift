import Foundation
import SubstrateSdk
import Operation_iOS
import SubstrateStorageQuery
import KeyDerivation
import FoundationExt
import ChainStore
import SDKLogger

public struct SSSFreeSlotInfo {
    public let period: UInt32
    public let seq: UInt32
    public let personOrigin: PersonOrigin

    public init(
        period: UInt32,
        seq: UInt32,
        personOrigin: PersonOrigin
    ) {
        self.period = period
        self.seq = seq
        self.personOrigin = personOrigin
    }
}

/// `nowSeconds` is the chain time the candidates were read at.
public struct SSSEvictSlotInfo {
    public let period: UInt32
    public let nowSeconds: UInt64
    public let candidates: [SSSEvictionCandidate]

    public init(
        period: UInt32,
        nowSeconds: UInt64,
        candidates: [SSSEvictionCandidate]
    ) {
        self.period = period
        self.nowSeconds = nowSeconds
        self.candidates = candidates
    }
}

public struct SSSRenewalSlot {
    public let personOrigin: PersonOrigin
    public let seq: UInt32

    public init(personOrigin: PersonOrigin, seq: UInt32) {
        self.personOrigin = personOrigin
        self.seq = seq
    }
}

public enum SSSSlotPlan {
    case free(SSSFreeSlotInfo)
    case evict(SSSEvictSlotInfo)
}

public protocol StatementStoreSlotInfoProviding {
    func hasExistingSlot(for accountId: AccountId) async throws -> Bool
    func slotPlan(
        excluding accountId: AccountId,
        callerPriority: AllowanceRecord.Priority
    ) async throws -> SSSSlotPlan
    func renewalSlots(period: UInt32) async throws -> [SSSRenewalSlot]
}

public final class StatementStoreSlotInfoProvider: StatementStoreSlotInfoProviding {
    private let chainId: ChainId
    private let chainRegistry: ChainResourceProtocol
    private let storageRequestFactory: StorageRequestFactoryProtocol
    private let networkSuffixProvider: NetworkSuffixProviding
    private let resourcesParameters: ResourcesParametersProviding
    private let chainTimeProvider: ChainTimeProviding
    private let originPersonProvider: OriginPersonProviding
    private let logger: SDKLoggerProtocol
    private let evictionPicker: StatementStoreSlotEvictionPicking

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        storageRequestFactory: StorageRequestFactoryProtocol,
        networkSuffixProvider: NetworkSuffixProviding,
        resourcesParameters: ResourcesParametersProviding,
        chainTimeProvider: ChainTimeProviding,
        originPersonProvider: OriginPersonProviding,
        accounting: StatementStoreSlotAccounting,
        logger: SDKLoggerProtocol
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.storageRequestFactory = storageRequestFactory
        self.networkSuffixProvider = networkSuffixProvider
        self.resourcesParameters = resourcesParameters
        self.chainTimeProvider = chainTimeProvider
        self.originPersonProvider = originPersonProvider
        self.logger = logger
        evictionPicker = StatementStoreSlotEvictionPicker(accounting: accounting)
    }

    public func hasExistingSlot(for accountId: AccountId) async throws -> Bool {
        let state = try await fetchState()
        return state.originEntries.contains { originEntry in
            originEntry.slots.entries.contains { $0.value?.accountId == accountId }
        }
    }

    public func renewalSlots(period: UInt32) async throws -> [SSSRenewalSlot] {
        let originEntries = try await fetchAllowanceEntries(period: period)

        return originEntries.flatMap { originEntry in
            originEntry.slots.entries.enumerated().compactMap { index, response in
                guard response.value == nil else { return nil }
                return SSSRenewalSlot(personOrigin: originEntry.personOrigin, seq: UInt32(index))
            }
        }
    }

    public func slotPlan(
        excluding accountId: AccountId,
        callerPriority: AllowanceRecord.Priority
    ) async throws -> SSSSlotPlan {
        let state = try await fetchState()

        for originEntry in state.originEntries {
            guard let freeIndex = originEntry.slots.entries.firstIndex(where: { $0.value == nil }) else {
                continue
            }
            logger.debug("Found free slot: \(originEntry.personOrigin.label) idx: \(freeIndex)")

            return .free(
                SSSFreeSlotInfo(
                    period: state.period,
                    seq: UInt32(freeIndex),
                    personOrigin: originEntry.personOrigin
                )
            )
        }

        let candidates = try await evictionPicker.rankCandidates(
            from: occupiedSlots(from: state),
            excluding: accountId,
            callerPriority: callerPriority
        )
        logger.debug("Found \(candidates.count) slots to evict for period \(state.period)")

        return .evict(SSSEvictSlotInfo(period: state.period, nowSeconds: state.nowSeconds, candidates: candidates))
    }
}

private extension StatementStoreSlotInfoProvider {
    typealias AllowanceEntries = [StorageResponse<ResourcesPallet.StmtStoreAllowanceEntry>]

    struct OriginSlots {
        let aliases: [Data]
        let entries: AllowanceEntries
    }

    struct OriginEntries {
        let personOrigin: PersonOrigin
        let slots: OriginSlots
    }

    struct SlotState {
        let originEntries: [OriginEntries]
        let period: UInt32
        let nowSeconds: UInt64
    }

    func fetchState() async throws -> SlotState {
        let nowSeconds = try await chainTimeProvider.nowSeconds()
        let period = UInt32(TimeInterval(nowSeconds) / .secondsInDay)

        let originEntries = try await fetchAllowanceEntries(period: period)

        return SlotState(originEntries: originEntries, period: period, nowSeconds: nowSeconds)
    }

    func fetchAllowanceEntries(
        period: UInt32
    ) async throws -> [OriginEntries] {
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
        let origins = try await originPersonProvider.pickPersonOrigins()

        let collected = try await withThrowingTaskGroup(
            of: (MembersPallet.CollectionIdentifier, OriginSlots).self
        ) { group in
            for origin in origins {
                group.addTask { [self] in
                    let collectionId = origin.collectionIdentifier
                    let slots = try await fetchSlots(
                        period: period,
                        personOrigin: origin,
                        codingFactory: codingFactory
                    )
                    return (collectionId, slots)
                }
            }
            var result: [MembersPallet.CollectionIdentifier: OriginSlots] = [:]
            for try await (collectionId, slots) in group {
                result[collectionId] = slots
            }
            return result
        }

        let originEntries = origins.compactMap { origin -> OriginEntries? in
            guard let slots = collected[origin.collectionIdentifier], !slots.entries.isEmpty else { return nil }
            return OriginEntries(personOrigin: origin, slots: slots)
        }
        guard !originEntries.isEmpty else { throw AllowanceSlotAssignmentError.noSlotsAvailable }

        return originEntries
    }

    func fetchSlots(
        period: UInt32,
        personOrigin: PersonOrigin,
        codingFactory: RuntimeCoderFactoryProtocol
    ) async throws -> OriginSlots {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
        let maxSlots = try await fetchMaxSlots(origin: personOrigin)
        guard maxSlots > 0 else { return OriginSlots(aliases: [], entries: []) }

        let networkSuffix = try await networkSuffixProvider.networkSuffix()

        let activeVrfManager = personOrigin.keyManager
        let aliases = try (0 ..< maxSlots).map { [networkSuffix] seq in
            let context = try ProductContextSuffix
                .statementStoreSlot(period: period, seq: seq)
                .context(networkSuffix: networkSuffix)
            return try activeVrfManager.deriveAlias(for: context)
        }

        let periodBytes = Data(period.bigEndianBytes)
        let entries: AllowanceEntries = try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams1: { Array(repeating: BytesCodable(wrappedValue: periodBytes), count: aliases.count) },
            keyParams2: { aliases.map { BytesCodable(wrappedValue: $0) } },
            factory: { codingFactory },
            storagePath: ResourcesPallet.statementStoreAllowances
        )
        .asyncExecute()

        return OriginSlots(aliases: aliases, entries: entries)
    }

    func fetchMaxSlots(origin: PersonOrigin) async throws -> UInt32 {
        try await resourcesParameters.stmtStoreSlotsPerPeriod(chainId: chainId, origin: origin)
    }

    func occupiedSlots(from state: SlotState) -> [SSSOccupiedSlot] {
        state.originEntries.flatMap { originEntry in
            originEntry.slots.entries.enumerated().compactMap { index, response in
                guard let entry = response.value else { return nil }
                return SSSOccupiedSlot(
                    personOrigin: originEntry.personOrigin,
                    seq: UInt32(index),
                    alias: originEntry.slots.aliases[index],
                    accountId: entry.accountId,
                    since: entry.since
                )
            }
        }
    }
}

private extension PersonOrigin {
    var label: String {
        switch self {
        case .lite: "lite"
        case .full: "full"
        }
    }
}
