import BackgroundExecution
import ChainRegistry
import ChainStore
import DurableTransactions
import ExtrinsicService
import Foundation
import os
import SubstrateSdk
import SubstrateSdkExt

/// The app's chain-bound extrinsic tools for the durability engine, resolved from the chain registry and
/// built once per chain and extrinsic format.
///
/// The format is decided by ``ExtrinsicVersionProviding`` on every request, from the runtime as it is
/// now, so a runtime upgrade mid-process switches to freshly built tools instead of reusing a format the
/// chain no longer takes (a signed V5 on a pipeline without `VerifyMultiSignature` would go out unsigned).
/// This is what the fee estimator does too, so the estimate and the submission agree. Durable
/// transactions on a chain in `signedChains` are signed by an account (installation registration on
/// Asset Hub); everywhere else they are general transactions carrying their own origin (coinage on
/// People). A chain hosting both kinds would need the format per request, which the engine does not
/// model yet.
final class DurableChainToolsProvider: DurableChainToolsProviding, @unchecked Sendable {
    struct Tools {
        let operationFactory: any ExtrinsicOperationFactoryProtocol
        let submitter: any ExtrinsicSubmitting
    }

    private let chainRegistry: ChainRegistryProtocol
    private let extrinsicFacade: ExtrinsicSubmissionMonitorFacade
    private let versionProvider: ExtrinsicVersionProviding
    private let signedChains: Set<ChainId>
    private let tools = OSAllocatedUnfairLock<[ToolsKey: Tools]>(initialState: [:])

    init(
        chainRegistry: ChainRegistryProtocol,
        extrinsicFacade: ExtrinsicSubmissionMonitorFacade,
        versionProvider: ExtrinsicVersionProviding,
        signedChains: Set<ChainId>
    ) {
        self.chainRegistry = chainRegistry
        self.extrinsicFacade = extrinsicFacade
        self.versionProvider = versionProvider
        self.signedChains = signedChains
    }

    func extrinsicOperationFactory(for chainId: ChainId) async throws -> any ExtrinsicOperationFactoryProtocol {
        try await tools(for: chainId).operationFactory
    }

    func extrinsicSubmitter(for chainId: ChainId) async throws -> any ExtrinsicSubmitting {
        try await tools(for: chainId).submitter
    }
}

private extension DurableChainToolsProvider {
    struct ToolsKey: Hashable {
        let chainId: ChainId
        let version: Extrinsic.Version
    }

    func tools(for chainId: ChainId) async throws -> Tools {
        guard let chain = chainRegistry.getChain(for: chainId) else {
            throw DurableTxError.chainViewUnavailable
        }

        let extrinsicVersion = try await versionProvider.getExtrinsicVersion(
            for: chainId,
            isSigned: signedChains.contains(chainId)
        )
        let key = ToolsKey(chainId: chainId, version: extrinsicVersion)

        if let cached = tools.withLock({ $0[key] }) {
            return cached
        }

        // Durability must observe the finalized outcome, not just inclusion, so the watch follows each
        // extrinsic until its block is finalized.
        let built = try Tools(
            operationFactory: extrinsicFacade.createOperationFactory(chain: chain, extrinsicVersion: extrinsicVersion),
            submitter: extrinsicFacade.makeForkProtectedSubmitter(
                chain: chain,
                trackingTill: .finalized,
                extrinsicVersion: extrinsicVersion
            )
        )

        return tools.withLock { current in
            if let existing = current[key] { return existing }
            current[key] = built
            return built
        }
    }
}
