import Foundation
import SubstrateSdk
import Operation_iOS
import SubstrateStorageQuery
import KeyDerivation
import FoundationExt
import ChainStore
import SDKLogger

public struct SSSSlotInfo {
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

public struct SSSRenewalSlot {
    public let personOrigin: PersonOrigin
    public let seq: UInt32

    public init(personOrigin: PersonOrigin, seq: UInt32) {
        self.personOrigin = personOrigin
        self.seq = seq
    }
}

public protocol StatementStoreSlotInfoProviding {
    func hasExistingSlot(for accountId: AccountId) async throws -> Bool
    func freeSlot(excluding accountId: AccountId, callerPriority: AllowanceRecord.Priority) async throws -> SSSSlotInfo
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
            originEntry.entries.contains { $0.value?.accountId == accountId }
        }
    }

    public func renewalSlots(period: UInt32) async throws -> [SSSRenewalSlot] {
        let originEntries = try await fetchAllowanceEntries(period: period)

        return originEntries.flatMap { originEntry in
            originEntry.entries.enumerated().compactMap { index, response in
                guard response.value == nil else { return nil }
                return SSSRenewalSlot(personOrigin: originEntry.personOrigin, seq: UInt32(index))
            }
        }
    }

    public func freeSlot(
        excluding accountId: AccountId,
        callerPriority: AllowanceRecord.Priority
    ) async throws -> SSSSlotInfo {
        let state = try await fetchState()

        for originEntry in state.originEntries {
            guard let freeIndex = originEntry.entries.firstIndex(where: { $0.value == nil }) else {
                continue
            }
            logger.debug("Found free slot: \(originEntry.personOrigin.label) idx: \(freeIndex)")

            return SSSSlotInfo(
                period: state.period,
                seq: UInt32(freeIndex),
                personOrigin: originEntry.personOrigin
            )
        }

        let cooldown = try await fetchReplacementCooldown()

        let oldest = try await evictionPicker.pickCandidate(
            from: occupiedSlots(from: state),
            excluding: accountId,
            callerPriority: callerPriority,
            cooldown: cooldown,
            nowSeconds: state.nowSeconds
        )
        logger.debug("Found slot to evict: \(oldest.personOrigin.label) seq: \(oldest.seq)")

        return SSSSlotInfo(
            period: state.period,
            seq: oldest.seq,
            personOrigin: oldest.personOrigin
        )
    }
}

private extension StatementStoreSlotInfoProvider {
    typealias AllowanceEntries = [StorageResponse<ResourcesPallet.StmtStoreAllowanceEntry>]

    struct OriginEntries {
        let personOrigin: PersonOrigin
        let entries: AllowanceEntries
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

        return SlotState(
            originEntries: originEntries,
            period: period,
            nowSeconds: nowSeconds
        )
    }

    func fetchAllowanceEntries(
        period: UInt32
    ) async throws -> [OriginEntries] {
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
        let origins = try await originPersonProvider.pickPersonOrigins()

        let collected = try await withThrowingTaskGroup(
            of: (MembersPallet.CollectionIdentifier, AllowanceEntries).self
        ) { group in
            for origin in origins {
                group.addTask { [self] in
                    let collectionId = origin.collectionIdentifier
                    let entries = try await fetchEntries(
                        period: period,
                        personOrigin: origin,
                        codingFactory: codingFactory
                    )
                    return (collectionId, entries)
                }
            }
            var result: [MembersPallet.CollectionIdentifier: AllowanceEntries] = [:]
            for try await (collectionId, entries) in group {
                result[collectionId] = entries
            }
            return result
        }

        let originEntries = origins.compactMap { origin -> OriginEntries? in
            guard let entries = collected[origin.collectionIdentifier], !entries.isEmpty else { return nil }
            return OriginEntries(personOrigin: origin, entries: entries)
        }
        guard !originEntries.isEmpty else { throw AllowanceSlotAssignmentError.noSlotsAvailable }

        return originEntries
    }

    func fetchEntries(
        period: UInt32,
        personOrigin: PersonOrigin,
        codingFactory: RuntimeCoderFactoryProtocol
    ) async throws -> AllowanceEntries {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
        let maxSlots = try await fetchMaxSlots(origin: personOrigin)
        guard maxSlots > 0 else { return [] }

        let networkSuffix = try await networkSuffixProvider.networkSuffix()

        let activeVrfManager = personOrigin.keyManager
        let aliases = try (0 ..< maxSlots).map { [networkSuffix] seq in
            let context = try ProductContextSuffix
                .statementStoreSlot(period: period, seq: seq)
                .context(networkSuffix: networkSuffix)
            return try activeVrfManager.deriveAlias(for: context)
        }

        let periodBytes = Data(period.bigEndianBytes)
        return try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams1: { Array(repeating: BytesCodable(wrappedValue: periodBytes), count: aliases.count) },
            keyParams2: { aliases.map { BytesCodable(wrappedValue: $0) } },
            factory: { codingFactory },
            storagePath: ResourcesPallet.statementStoreAllowances
        )
        .asyncExecute()
    }

    func fetchMaxSlots(origin: PersonOrigin) async throws -> UInt32 {
        try await resourcesParameters.stmtStoreSlotsPerPeriod(chainId: chainId, origin: origin)
    }

    func fetchReplacementCooldown() async throws -> UInt32 {
        try await resourcesParameters.stmtStoreReplacementCooldown(chainId: chainId)
    }

    func occupiedSlots(from state: SlotState) -> [SSSOccupiedSlot] {
        state.originEntries.flatMap { originEntry in
            originEntry.entries.enumerated().compactMap { index, response in
                guard let entry = response.value else { return nil }
                return SSSOccupiedSlot(
                    personOrigin: originEntry.personOrigin,
                    seq: UInt32(index),
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
