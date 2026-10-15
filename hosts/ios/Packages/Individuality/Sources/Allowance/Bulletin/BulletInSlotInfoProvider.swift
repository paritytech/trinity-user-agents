import Foundation
import ChainStore
import KeyDerivation
import SubstrateStorageQuery
import SubstrateStorageSubscription
import Operation_iOS
import BulletinChain
import SubstrateSdk
import SubstrateSdkExt
import SubstrateOperation
import StructuredConcurrency
import AsyncExtensions
import IssueMonitoring

public struct BulletInFreeSlotInfo {
    public let counter: UInt8
    public let period: UInt32
    public let personOrigin: PersonOrigin
}

public struct BulletInAllowanceInfo {
    public let remainedSize: UInt64
    public let remainedTransactions: UInt32
    public let expiresIn: BlockNumber
    public let fetchedAt: BlockNumber

    public var isExpired: Bool {
        fetchedAt >= expiresIn
    }

    public var available: Bool {
        remainedSize > 0 && remainedTransactions > 0 && !isExpired
    }

    public init(
        remainedSize: UInt64,
        remainedTransactions: UInt32,
        expiresIn: BlockNumber,
        fetchedAt: BlockNumber
    ) {
        self.remainedSize = remainedSize
        self.remainedTransactions = remainedTransactions
        self.expiresIn = expiresIn
        self.fetchedAt = fetchedAt
    }
}

public protocol BulletInSlotInfoProviding {
    func fetchFreeSlotInfo() async throws -> BulletInFreeSlotInfo
    func fetchAllowance(for accountId: AccountId) async throws -> BulletInAllowanceInfo?
    func waitAuthorization(
        for accountId: AccountId,
        currentAllowance: BulletInAllowanceInfo?,
        timeout: Duration
    ) async throws
    func canAccountPromote(for accountId: AccountId) async throws -> Bool
    func waitSubmittable(for accountId: AccountId, timeout: Duration) async throws
}

enum BulletInSlotInfoProviderError: Error {
    case noAuthorization
    case authorizationWaitTimeout
}

extension BulletInSlotInfoProviderError: ReportableError {
    var reportCode: String { "\(self)" }
}

public final class BulletInSlotInfoProvider {
    let bulletInChainId: ChainId
    let peopleChainId: ChainId
    let chainRegistry: ChainResourceProtocol
    let keyResolver: BandersnatchKeyResolving
    let bulletInBlockProvider: BlockInfoProviding
    let operationQueue: OperationQueue
    let chainTimeProvider: ChainTimeProviding

    let storageRequestFactory: StorageRequestFactoryProtocol
    let networkSuffixProvider: NetworkSuffixProviding
    let resourcesParameters: ResourcesParametersProviding
    let hopRuntimeApi: HopRuntimeApiProtocol

    public init(
        bulletInChainId: ChainId,
        peopleChainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        keyResolver: BandersnatchKeyResolving,
        operationQueue: OperationQueue,
        chainTimeProvider: ChainTimeProviding,
        resourcesParameters: ResourcesParametersProviding,
        networkSuffixProvider: NetworkSuffixProviding
    ) {
        self.bulletInChainId = bulletInChainId
        self.peopleChainId = peopleChainId
        self.chainRegistry = chainRegistry
        self.keyResolver = keyResolver
        self.operationQueue = operationQueue
        self.chainTimeProvider = chainTimeProvider
        self.resourcesParameters = resourcesParameters
        self.networkSuffixProvider = networkSuffixProvider
        bulletInBlockProvider = BlockInfoProvider(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            chainId: bulletInChainId
        )

        storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        hopRuntimeApi = HopRuntimeApi(chainRegistry: chainRegistry, operationQueue: operationQueue)
    }
}

extension BulletInSlotInfoProvider: BulletInSlotInfoProviding {
    public func fetchFreeSlotInfo() async throws -> BulletInFreeSlotInfo {
        let peopleConnection = try chainRegistry.getRpcConnectionOrError(for: peopleChainId)
        let peopleRuntimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: peopleChainId)
        let codingFactory = try await peopleRuntimeProvider.fetchCoderFactoryOperation().asyncExecute()
        let periodDuration = try await fetchPeriodDuration(codingFactory: codingFactory)

        let personOrigin = try await OriginPersonProvider(
            liteVrfManager: keyResolver.liteKeyManager,
            liteCollectionId: PeopleLitePallet.membersIdentifier,
            fullVrfManager: keyResolver.fullKeyManager,
            fullCollectionId: PeoplePallet.membersIdentifier,
            connection: peopleConnection,
            runtimeCodingService: peopleRuntimeProvider
        ).pickPersonOrigin()

        let maxClaims = try await fetchMaxClaims()

        guard maxClaims > 0, periodDuration > 0 else {
            throw AllowanceSlotAssignmentError.noSlotsAvailable
        }

        let nowSeconds = try await chainTimeProvider.nowSeconds()
        let period = UInt32(TimeInterval(nowSeconds) / TimeInterval(periodDuration))

        let networkSuffix = try await networkSuffixProvider.networkSuffix()

        let aliases = try (0 ..< maxClaims).map { [networkSuffix] counter -> Data in
            let context = try ProductContextSuffix
                .longTermStorage(period: period, counter: counter)
                .context(networkSuffix: networkSuffix)
            return try personOrigin.keyManager.deriveAlias(for: context)
        }

        let periodBytes = Data(period.bigEndianBytes)

        let responses: [StorageResponse<JSON>] =
            try await storageRequestFactory.queryItems(
                engine: peopleConnection,
                keyParams1: { aliases.map { _ in BytesCodable(wrappedValue: periodBytes) } },
                keyParams2: { aliases.map { BytesCodable(wrappedValue: $0) } },
                factory: { codingFactory },
                storagePath: ResourcesPallet.spentLongTermStorageAliases
            )
            .asyncExecute()

        guard let index = responses.firstIndex(where: { $0.data == nil }) else {
            throw AllowanceSlotAssignmentError.noSlotsAvailable
        }

        return BulletInFreeSlotInfo(
            counter: UInt8(index),
            period: period,
            personOrigin: personOrigin
        )
    }

    public func fetchAllowance(for accountId: AccountId) async throws -> BulletInAllowanceInfo? {
        let connection = try chainRegistry.getRpcConnectionOrError(for: bulletInChainId)
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: bulletInChainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        let authorizations: TransactionStoragePallet.Authorization? = try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams: {
                [TransactionStoragePallet.AuthorizationScope.account(accountId)]
            },
            factory: { codingFactory },
            storagePath: TransactionStoragePallet.authorizationsPath
        )
        .asyncExecute()
        .first?.value

        guard let authorizations else {
            return nil
        }

        let blockNumber: BlockNumber = try await bulletInBlockProvider.fetchCurrent()

        return BulletInAllowanceInfo(
            remainedSize: authorizations.extent.remainedBytes,
            remainedTransactions: authorizations.extent.remainedTransactions,
            expiresIn: authorizations.expiration,
            fetchedAt: blockNumber
        )
    }

    public func waitAuthorization(
        for accountId: AccountId,
        currentAllowance: BulletInAllowanceInfo?,
        timeout: Duration
    ) async throws {
        let connection = try chainRegistry.getRpcConnectionOrError(for: bulletInChainId)
        let runtimeService = try chainRegistry.getRuntimeCodingServiceOrError(for: bulletInChainId)

        let request = MapSubscriptionRequest(
            storagePath: TransactionStoragePallet.authorizationsPath,
            localKey: "",
            keyParamClosure: {
                TransactionStoragePallet.AuthorizationScope.account(accountId)
            }
        )

        let batchRequest = BatchStorageSubscriptionRequest(
            innerRequest: request,
            mappingKey: nil
        )

        let stream: AnyAsyncSequence<BatchStorageSubscriptionSingleResult<TransactionStoragePallet.Authorization?>>
        stream = CallbackBatchStorageSubscription.asyncStream(
            requests: [batchRequest],
            connection: connection,
            runtimeService: runtimeService,
            logger: nil
        )

        do {
            let currentRemainedTransactions = currentAllowance?.remainedTransactions ?? 0
            try await withTimeout(timeout) {
                for try await result in stream {
                    if
                        let authorization = result.value,
                        authorization.extent.remainedTransactions > currentRemainedTransactions {
                        return
                    }
                }
            }
        } catch is TimeoutError {
            throw BulletInSlotInfoProviderError.authorizationWaitTimeout
        }
    }

    public func canAccountPromote(for accountId: AccountId) async throws -> Bool {
        try await hopRuntimeApi.canAccountPromote(
            chainId: bulletInChainId,
            account: accountId
        )
    }

    /// Re-evaluates `can_account_promote` immediately and then on each new Bulletin head until it returns
    /// `true` or [timeout] elapses. The head stream is only a wakeup, so this does not depend on the stored
    /// authorization extent layout.
    public func waitSubmittable(for accountId: AccountId, timeout: Duration) async throws {
        let newHeads = bulletInBlockProvider.subscribeNewHeads()

        do {
            try await withTimeout(timeout) { [self] in
                if try await canAccountPromote(for: accountId) {
                    return
                }

                for try await _ in newHeads {
                    if try await canAccountPromote(for: accountId) {
                        return
                    }
                }
            }
        } catch is TimeoutError {
            throw BulletInSlotInfoProviderError.authorizationWaitTimeout
        }
    }
}

private extension BulletInSlotInfoProvider {
    func fetchMaxClaims() async throws -> UInt8 {
        try await resourcesParameters.longTermStorageClaimsPerPeriod(chainId: peopleChainId)
    }

    func fetchPeriodDuration(codingFactory: RuntimeCoderFactoryProtocol) async throws -> UInt32 {
        let operation = StorageConstantOperation<StringCodable<UInt32>>(
            path: ResourcesPallet.Constants.longTermStoragePeriodDuration(),
            fallbackValue: nil
        )
        operation.codingFactory = codingFactory
        return try await operation.asyncExecute().wrappedValue
    }
}
