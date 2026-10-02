import Foundation
import ExtrinsicService
import SubstrateSdk
import Individuality
import KeyDerivation
import ChainStore
import Coinage
import ChainRegistry
import StructuredConcurrency

/// App-side factory implementing OriginCreating.
/// Provides origin definitions for extrinsics with optional ring membership proofs for both full and lite people paths.
final class CoinageOriginFactory: ExtrinsicOriginFactory, OriginCreating {
    let chain: ChainProtocol
    private let connection: JSONRPCEngine

    // ring membership
    private let voucherKeyFactory: any VoucherKeyDeriving

    private let fullPersonProofParamsFactory: RingProofParamsProviderMaking
    private let lightPersonProofParamsFactory: RingProofParamsProviderMaking

    private let proofParamsFetcher: MembershipProofParamsFetching

    // unload tokens
    private let unloadTokenResolver: UnloadTokenResolving

    let personOriginProvider: OriginPersonProviding

    init(
        chainRegistry: ChainRegistryProtocol,
        operationQueue: OperationQueue,
        chain: ChainProtocol,
        voucherKeyFactory: any VoucherKeyDeriving,
        fullPersonKeyManager: BandersnatchKeyManaging,
        lightPersonKeyManager: BandersnatchKeyManaging,
        unloadTokenResolver: UnloadTokenResolving,
        connection: JSONRPCEngine,
        runtimeCodingService: RuntimeCodingServiceProtocol,
        logger: LoggerProtocol,
    ) {
        self.chain = chain
        self.connection = connection
        self.voucherKeyFactory = voucherKeyFactory
        self.unloadTokenResolver = unloadTokenResolver

        proofParamsFetcher = MembershipProofParamsFetcher(
            connection: connection,
            runtimeCodingService: runtimeCodingService
        )

        fullPersonProofParamsFactory = RingProofParamsProviderFactory(
            collectionIdentifier: PeoplePallet.membersIdentifier,
            proofParamsFetcher: proofParamsFetcher
        )

        lightPersonProofParamsFactory = RingProofParamsProviderFactory(
            collectionIdentifier: PeopleLitePallet.membersIdentifier,
            proofParamsFetcher: proofParamsFetcher
        )

        personOriginProvider = OriginPersonProvider(
            liteVrfManager: lightPersonKeyManager,
            liteCollectionId: PeopleLitePallet.membersIdentifier,
            fullVrfManager: fullPersonKeyManager,
            fullCollectionId: PeoplePallet.membersIdentifier,
            connection: connection,
            runtimeCodingService: runtimeCodingService
        )

        super.init(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            logger: logger
        )
    }

    func createAsCoinOrigin(for wallet: WalletManaging) throws -> ExtrinsicOriginDefining {
        let signedOrigin = try createSignedOrigin(
            for: wallet,
            chain: chain
        )

        let asCoinOrigin = AsCoinOriginDefinition()

        return ExtrinsicCompoundOrigin(
            children: [
                asCoinOrigin,
                signedOrigin
            ]
        )
    }

    func createSignedOrigin(for wallet: WalletManaging, chainId: ChainId) async throws -> ExtrinsicOriginDefining {
        try createSignedOrigin(for: wallet, chain: chainRegistry.getChainOrError(for: chainId))
    }

    func createInfallibleUnpaidSignedOrigin(for wallet: WalletManaging) throws -> ExtrinsicOriginDefining {
        let infallibleOrigin = InfallibleUnpaidSignedOriginDefinition()

        let accountOrigin = try createAccountOrigin(for: wallet, chain: chain)

        let signedOrigin = try createSigningByAccountOrigin(for: wallet, chain: chain)

        let restrictionOrigin = RestrictsOriginDefinition(enabled: false)

        return ExtrinsicCompoundOrigin(
            children: [
                accountOrigin,
                restrictionOrigin,
                infallibleOrigin,
                signedOrigin
            ]
        )
    }

    func createAsUnloadTokenOrigins(
        voucherGroups: [[Voucher]],
        currentDate: Date,
        blockHash: BlockHashData?
    ) async throws -> [ExtrinsicOriginDefining] {
        guard !voucherGroups.isEmpty else { return [] }

        let personOrigin = try await personOriginProvider.pickPersonOrigin()

        let aliasProvider = personOrigin.makeAliasProvider()
        let personProofDependency = makeProofDependency(for: personOrigin)

        let resolvedTokens = try await markStallRegion("Resolve unload tokens") {
            try await unloadTokenResolver.resolve(
                groups: voucherGroups,
                aliasProvider: aliasProvider,
                currentDate: currentDate
            )
        }

        return try zip(voucherGroups, resolvedTokens).map { group, resolvedToken in
            let voucherKeyManagers = try group.map {
                try voucherKeyFactory.createKeyManager(for: $0)
            }

            let originDefinition: ExtrinsicOriginDefining

            let input = AsUnloadTokenPeopleInput(
                personDeps: personProofDependency,
                currentDate: currentDate,
                resolvedToken: resolvedToken
            )

            originDefinition = try AsUnloadTokenPeopleOriginDefinition(
                input: input,
                voucherKeyManagers: voucherKeyManagers,
                recyclerRingMemberProvider: makeRecyclerProofParamsProvider(for: group),
                blockHash: blockHash
            )

            return ExtrinsicCompoundOrigin(
                children: [
                    RestrictsOriginDefinition(enabled: false),
                    originDefinition
                ]
            )
        }
    }
}

// MARK: - Private

private extension CoinageOriginFactory {
    /// Creates a ring member provider for the recycler represented by the given vouchers.
    func makeRecyclerProofParamsProvider(
        for vouchers: [Voucher]
    ) throws -> any RingProofParamsProviding {
        guard let firstVoucher = vouchers.first,
              let recycler = firstVoucher.recycler else {
            throw CoinageCommonError.recyclerNotFound
        }

        let key = RecyclerKey(exponent: firstVoucher.exponent, index: recycler.index)

        return RecyclerProofParamsProvider(
            instanceId: AppConfig.Coinage.instanceId,
            recyclerKey: key,
            proofParamsFetcher: proofParamsFetcher
        )
    }

    func makeProofDependency(for pickedOrigin: PersonOrigin) -> PersonProofDependency {
        switch pickedOrigin {
        case let .lite(ringIndex, keyManager):
            PersonProofDependency(
                origin: pickedOrigin,
                keyManager: keyManager,
                proofParamsFetcher: lightPersonProofParamsFactory.createProvider(for: ringIndex)
            )
        case let .full(ringIndex, keyManager):
            PersonProofDependency(
                origin: pickedOrigin,
                keyManager: keyManager,
                proofParamsFetcher: fullPersonProofParamsFactory.createProvider(for: ringIndex)
            )
        }
    }
}
