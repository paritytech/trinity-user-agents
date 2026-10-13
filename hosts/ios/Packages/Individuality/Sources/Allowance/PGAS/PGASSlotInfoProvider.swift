import Foundation
import SubstrateSdk
import Operation_iOS
import SubstrateStorageQuery
import KeyDerivation
import FoundationExt
import ChainStore

public protocol PGASSlotInfoProviding {
    func hasExistingSlot(for account: AccountId) async throws -> Bool
    func freeSlot() async throws -> PGASSlotInfo
}

public final class PGASSlotInfoProvider: PGASSlotInfoProviding {
    private let chainId: ChainId
    private let peopleChainId: ChainId
    private let chainRegistry: ChainResourceProtocol
    private let storageRequestFactory: StorageRequestFactoryProtocol
    private let networkSuffixProvider: NetworkSuffixProviding
    private let keyResolver: BandersnatchKeyResolving
    private let chainTimeProvider: ChainTimeProviding

    public init(
        chainId: ChainId,
        peopleChainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        storageRequestFactory: StorageRequestFactoryProtocol,
        networkSuffixProvider: NetworkSuffixProviding,
        keyResolver: BandersnatchKeyResolving,
        chainTimeProvider: ChainTimeProviding
    ) {
        self.chainId = chainId
        self.peopleChainId = peopleChainId
        self.chainRegistry = chainRegistry
        self.storageRequestFactory = storageRequestFactory
        self.networkSuffixProvider = networkSuffixProvider
        self.keyResolver = keyResolver
        self.chainTimeProvider = chainTimeProvider
    }

    public func hasExistingSlot(for _: AccountId) async throws -> Bool {
        let (entries, _, _) = try await fetchState()
        return entries.contains { $0.data != nil }
    }

    public func freeSlot() async throws -> PGASSlotInfo {
        let (entries, personOrigin, day) = try await fetchState()

        guard let freeIndex = entries.firstIndex(where: { $0.data == nil }) else {
            throw AllowanceSlotAssignmentError.noSlotsAvailable
        }

        return PGASSlotInfo(
            day: day,
            slotIndex: UInt32(freeIndex),
            personOrigin: personOrigin
        )
    }
}

private extension PGASSlotInfoProvider {
    func fetchState() async throws -> (
        entries: [StorageResponse<JSON>],
        personOrigin: PersonOrigin,
        day: UInt32
    ) {
        let day = try await chainTimeProvider.currentPeriod()

        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)

        let peopleRuntimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: peopleChainId)
        let peopleConnection = try chainRegistry.getRpcConnectionOrError(for: peopleChainId)

        let personOrigin = try await OriginPersonProvider(
            liteVrfManager: keyResolver.liteKeyManager,
            liteCollectionId: PeopleLitePallet.membersIdentifier,
            fullVrfManager: keyResolver.fullKeyManager,
            fullCollectionId: PeoplePallet.membersIdentifier,
            connection: peopleConnection,
            runtimeCodingService: peopleRuntimeProvider
        ).pickPersonOrigin()

        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
        let maxClaims = try await fetchMaxClaims(origin: personOrigin, codingFactory: codingFactory)
        guard maxClaims > 0 else { throw AllowanceSlotAssignmentError.noSlotsAvailable }

        let networkSuffix = try await networkSuffixProvider.networkSuffix()

        let activeVrfManager = personOrigin.keyManager
        let aliases = try (0 ..< maxClaims).map { [networkSuffix] slotIndex in
            let context = try ProductContextSuffix
                .pgasClaim(day: day, slot: slotIndex)
                .context(networkSuffix: networkSuffix)
            return try activeVrfManager.deriveAlias(for: context)
        }

        let dayBytes = Data(day.bigEndianBytes)
        let entries: [StorageResponse<JSON>] =
            try await storageRequestFactory.queryItems(
                engine: connection,
                keyParams1: { aliases.map { _ in BytesCodable(wrappedValue: dayBytes) } },
                keyParams2: { aliases.map { BytesCodable(wrappedValue: $0) } },
                factory: { codingFactory },
                storagePath: PGASPallet.claimedGasAliases
            )
            .asyncExecute()

        return (entries, personOrigin, day)
    }

    func fetchMaxClaims(
        origin: PersonOrigin,
        codingFactory: RuntimeCoderFactoryProtocol
    ) async throws -> UInt32 {
        let path =
            switch origin {
            case .lite: PGASPallet.Constants.maxClaimsPerPeriodPerLitePerson
            case .full: PGASPallet.Constants.maxClaimsPerPeriodPerPerson
            }
        let operation = StorageConstantOperation<StringCodable<UInt32>>(
            path: path(),
            fallbackValue: .init(wrappedValue: 0)
        )
        operation.codingFactory = codingFactory
        return try await operation.asyncExecute().wrappedValue
    }
}
