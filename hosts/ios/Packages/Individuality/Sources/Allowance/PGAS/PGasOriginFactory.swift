import ChainStore
import ExtrinsicService
import Foundation
import KeyDerivation
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateStorageSubscription
import StructuredConcurrency
import SubstrateSdkExt
import AsyncExtensions

public enum PGasOriginError: Error {
    case revisionPruned(UInt32)
    case revisionTimeout(UInt32)
}

public protocol PGasOriginCreating {
    func createPGASOrigin(
        personOrigin: PersonOrigin,
        day: UInt32,
        slotIndex: UInt32,
        peopleChainId: ChainId,
        submissionChainId: ChainId
    ) async throws -> ExtrinsicOriginDefining
}

public final class PGasOriginFactory {
    private static let revisionWaitTimeout: Duration = .seconds(30)

    private let keyResolver: BandersnatchKeyResolving
    private let chainRegistry: ChainResourceProtocol
    private let storageRequestFactory: StorageRequestFactoryProtocol
    private let networkSuffixProvider: NetworkSuffixProviding

    public init(
        keyResolver: BandersnatchKeyResolving,
        chainRegistry: ChainResourceProtocol,
        storageRequestFactory: StorageRequestFactoryProtocol,
        networkSuffixProvider: NetworkSuffixProviding
    ) {
        self.keyResolver = keyResolver
        self.chainRegistry = chainRegistry
        self.storageRequestFactory = storageRequestFactory
        self.networkSuffixProvider = networkSuffixProvider
    }
}

// MARK: - PGasOriginCreating

extension PGasOriginFactory: PGasOriginCreating {
    public func createPGASOrigin(
        personOrigin: PersonOrigin,
        day: UInt32,
        slotIndex: UInt32,
        peopleChainId: ChainId,
        submissionChainId: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        let personDeps = try await makePersonDeps(
            personOrigin: personOrigin,
            chain: peopleChainId
        )

        let networkSuffix = try await networkSuffixProvider.networkSuffix()
        let proofContext = try ProductContextSuffix
            .pgasClaim(day: day, slot: slotIndex)
            .context(networkSuffix: networkSuffix)

        let proofParams = try await fetchProofParams(
            personDeps: personDeps,
            chain: peopleChainId
        )

        try await awaitRingRevision(
            submissionChainId: submissionChainId,
            collectionId: personDeps.origin.collectionIdentifier,
            ringIndex: personDeps.origin.ringIndex,
            revision: proofParams.revision
        )

        let pgasOrigin = AsPgasOriginDefinition(
            input: AsPgasOriginInput(
                personDeps: personDeps,
                proofContext: proofContext,
                day: day,
                proofParams: proofParams
            )
        )

        let origin = RestrictsOriginDefinition(enabled: false)

        return ExtrinsicCompoundOrigin(children: [origin, pgasOrigin])
    }
}

// MARK: - Private

private extension PGasOriginFactory {
    func makePersonDeps(
        personOrigin: PersonOrigin,
        chain: ChainId
    ) async throws -> PersonProofDependency {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chain)
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chain)

        let proofParamsFetcher = MembershipProofParamsFetcher(
            connection: connection,
            runtimeCodingService: runtimeProvider
        )

        let paramsProvider = RingProofParamsProviderFactory(
            collectionIdentifier: personOrigin.collectionIdentifier,
            proofParamsFetcher: proofParamsFetcher
        ).createProvider(for: personOrigin.ringIndex)

        return PersonProofDependency(
            origin: personOrigin,
            keyManager: personOrigin.keyManager,
            proofParamsFetcher: paramsProvider
        )
    }

    /// Ring members and revision read at one People block, so the proof covers
    /// exactly the members of the revision it names.
    func fetchProofParams(
        personDeps: PersonProofDependency,
        chain: ChainId
    ) async throws -> MembersProofParams {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chain)
        let blockHash = try await JSONRPCListOperation<String>(
            engine: connection,
            method: RPCMethod.getBlockHash
        )
        .asyncExecute()

        return try await personDeps.proofParamsFetcher.fetchParams(
            blockHash: Data(hexString: blockHash)
        )
    }

    func fetchCurrentGeneration(
        connection: JSONRPCEngine,
        runtimeService: RuntimeCodingServiceProtocol
    ) async throws -> MembersSubscriberPallet.Generation {
        let codingFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()
        let generationPath = MembersSubscriberPallet.Storage.currentGeneration()
        let response: StorageResponse<StringCodable<MembersSubscriberPallet.Generation>> =
            try await storageRequestFactory
                .queryItem(
                    engine: connection,
                    factory: { codingFactory },
                    storagePath: generationPath
                )
                .asyncExecute()

        return response.value?.wrappedValue ?? 0
    }

    func awaitRingRevision(
        submissionChainId: ChainId,
        collectionId: Data,
        ringIndex: MembersPallet.RingIndex,
        revision: UInt32
    ) async throws {
        let connection = try chainRegistry.getRpcConnectionOrError(for: submissionChainId)
        let runtimeService = try chainRegistry.getRuntimeCodingServiceOrError(for: submissionChainId)

        let generation = try await fetchCurrentGeneration(connection: connection, runtimeService: runtimeService)

        let request = NMapSubscriptionRequest(
            storagePath: MembersSubscriberPallet.Storage.ringRoots(),
            localKey: "",
            keyParamClosure: {
                MembersSubscriberPallet.RingRootsKey(
                    generation: generation,
                    collectionId: collectionId,
                    ringIndex: ringIndex
                )
            }
        )

        let batchRequest = BatchStorageSubscriptionRequest(
            innerRequest: request,
            mappingKey: nil
        )

        typealias Records = [MembersSubscriberPallet.RingCommitmentRecord]?
        let stream: AnyAsyncSequence<BatchStorageSubscriptionSingleResult<Records>>
        stream = CallbackBatchStorageSubscription.asyncStream(
            requests: [batchRequest],
            connection: connection,
            runtimeService: runtimeService,
            logger: nil
        )

        do {
            try await withTimeout(Self.revisionWaitTimeout) {
                for try await result in stream {
                    let records = result.value ?? []

                    if records.contains(where: { $0.revision == revision }) {
                        return
                    }

                    // A newer root means this revision was evicted or skipped.
                    if let newest = records.map(\.revision).max(), newest > revision {
                        throw PGasOriginError.revisionPruned(revision)
                    }
                }
            }
        } catch is TimeoutError {
            throw PGasOriginError.revisionTimeout(revision)
        }
    }
}
