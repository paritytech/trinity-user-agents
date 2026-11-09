import Foundation
import SubstrateSdk
import ChainStore
import SubstrateOperation
import StructuredConcurrency

public struct MembershipDeps {
    public let statusChecker: MembershipStatusChecking
    public let proofParamsFetcher: MembershipProofParamsFetching
    public let blockInfoProvider: BlockInfoProviding

    public init(
        statusChecker: MembershipStatusChecking,
        proofParamsFetcher: MembershipProofParamsFetching,
        blockInfoProvider: BlockInfoProviding
    ) {
        self.statusChecker = statusChecker
        self.proofParamsFetcher = proofParamsFetcher
        self.blockInfoProvider = blockInfoProvider
    }
}

/// Resolves membership dependencies for a chain hosting the Members pallet.
/// Returns nil when the chain is unknown, has no Members pallet, or the requested
/// pallet index doesn't match the runtime metadata.
public protocol MembershipDepsResolving {
    func resolve(genesisHash: ChainId, palletIndex: UInt8?) async throws -> MembershipDeps?

    /// Checks the ring location without constructing dependencies, using
    /// the same chain and pallet-index rules as ``resolve(genesisHash:palletIndex:)``.
    func validate(genesisHash: ChainId, palletIndex: UInt8?) async throws -> Bool

    func membersPalletIndex(genesisHash: ChainId) async throws -> UInt8?
}

public final class MembershipDepsResolver {
    private let chainRegistry: ChainResourceProtocol
    private let operationQueue: OperationQueue

    public init(chainRegistry: ChainResourceProtocol, operationQueue: OperationQueue) {
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
    }
}

extension MembershipDepsResolver: MembershipDepsResolving {
    public func resolve(genesisHash: ChainId, palletIndex: UInt8?) async throws -> MembershipDeps? {
        guard let chain = try await validatedChain(genesisHash: genesisHash, palletIndex: palletIndex) else {
            return nil
        }

        return MembershipDeps(
            statusChecker: MembershipStatusChecker(
                connection: chain.connection,
                runtimeCodingService: chain.runtimeCodingService
            ),
            proofParamsFetcher: MembershipProofParamsFetcher(
                connection: chain.connection,
                runtimeCodingService: chain.runtimeCodingService
            ),
            blockInfoProvider: BlockInfoProvider(
                chainRegistry: chainRegistry,
                operationQueue: operationQueue,
                chainId: chain.chainId
            )
        )
    }

    public func validate(genesisHash: ChainId, palletIndex: UInt8?) async throws -> Bool {
        try await validatedChain(genesisHash: genesisHash, palletIndex: palletIndex) != nil
    }

    public func membersPalletIndex(genesisHash: ChainId) async throws -> UInt8? {
        guard
            let chainId = chainRegistry.getChainInterfaceByGenesis(genesisHash)?.chainId,
            let runtimeCodingService = chainRegistry.getRuntimeCodingService(for: chainId)
        else {
            return nil
        }

        return try await membersPalletIndex(runtimeCodingService: runtimeCodingService)
    }
}

private extension MembershipDepsResolver {
    struct ValidatedChain {
        let chainId: ChainId
        let connection: JSONRPCEngine
        let runtimeCodingService: RuntimeCodingServiceProtocol
    }

    func validatedChain(genesisHash: ChainId, palletIndex: UInt8?) async throws -> ValidatedChain? {
        guard
            let chainId = chainRegistry.getChainInterfaceByGenesis(genesisHash)?.chainId,
            let connection = chainRegistry.getRpcConnection(for: chainId),
            let runtimeCodingService = chainRegistry.getRuntimeCodingService(for: chainId)
        else {
            return nil
        }

        guard let membersIndex = try await membersPalletIndex(runtimeCodingService: runtimeCodingService) else {
            return nil
        }

        if let palletIndex, palletIndex != membersIndex {
            return nil
        }

        return ValidatedChain(
            chainId: chainId,
            connection: connection,
            runtimeCodingService: runtimeCodingService
        )
    }

    func membersPalletIndex(runtimeCodingService: RuntimeCodingServiceProtocol) async throws -> UInt8? {
        let codingFactory = try await runtimeCodingService.fetchCoderFactoryOperation().asyncExecute()
        return codingFactory.metadata.getModuleIndex(MembersPallet.name)
    }
}
