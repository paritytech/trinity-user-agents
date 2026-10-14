import Foundation
import ExtrinsicService
import SubstrateSdk
import SubstrateStorageQuery
import KeyDerivation
import ChainStore

public protocol AsResourcesOriginCreating {
    func createSSSOrigin(
        personOrigin: PersonOrigin,
        period: UInt32,
        seq: UInt32,
        chain: ChainId
    ) async throws -> ExtrinsicOriginDefining

    func createLTSOrigin(
        personOrigin: PersonOrigin,
        period: UInt32,
        counter: UInt8,
        chain: ChainId
    ) async throws -> ExtrinsicOriginDefining

    func createNotificationOrigin(
        personOrigin: PersonOrigin,
        period: UInt32,
        seq: UInt8,
        chain: ChainId
    ) async throws -> ExtrinsicOriginDefining
}

public final class AsResourcesOriginFactory: AsResourcesOriginCreating {
    private let wallet: WalletManaging
    private let keyResolver: BandersnatchKeyResolving
    private let chainRegistry: ChainResourceProtocol
    private let networkSuffixProvider: NetworkSuffixProviding

    public init(
        wallet: WalletManaging,
        keyResolver: BandersnatchKeyResolving,
        chainRegistry: ChainResourceProtocol,
        networkSuffixProvider: NetworkSuffixProviding
    ) {
        self.wallet = wallet
        self.keyResolver = keyResolver
        self.chainRegistry = chainRegistry
        self.networkSuffixProvider = networkSuffixProvider
    }

    public func createSSSOrigin(
        personOrigin: PersonOrigin,
        period: UInt32,
        seq: UInt32,
        chain: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        try await createOrigin(
            personOrigin: personOrigin,
            suffix: .statementStoreSlot(period: period, seq: seq),
            kind: .registerStatementStoreAllowance,
            chain: chain
        )
    }

    public func createLTSOrigin(
        personOrigin: PersonOrigin,
        period: UInt32,
        counter: UInt8,
        chain: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        try await createOrigin(
            personOrigin: personOrigin,
            suffix: .longTermStorage(period: period, counter: counter),
            kind: .claimLongTermStorage,
            chain: chain
        )
    }

    public func createNotificationOrigin(
        personOrigin: PersonOrigin,
        period: UInt32,
        seq: UInt8,
        chain: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        try await createOrigin(
            personOrigin: personOrigin,
            suffix: .notificationSlot(period: period, seq: seq),
            kind: .registerNotificationForCollection,
            chain: chain
        )
    }
}

private extension AsResourcesOriginFactory {
    func createOrigin(
        personOrigin: PersonOrigin,
        suffix: ProductContextSuffix,
        kind: AsResourcesOriginInput.Kind,
        chain: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        let personDeps = try await makePersonDeps(personOrigin: personOrigin, chain: chain)
        let proofContext = try await suffix.context(networkSuffix: networkSuffixProvider.networkSuffix())

        let asResourcesOrigin = AsResourcesOriginDefinition(
            input: AsResourcesOriginInput(personDeps: personDeps, proofContext: proofContext, kind: kind)
        )

        return ExtrinsicCompoundOrigin(children: [RestrictsOriginDefinition(enabled: false), asResourcesOrigin])
    }

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
}
