import Foundation
import ChainStore
import ChainRegistry
import Operation_iOS
import SubstrateSdk
import StructuredConcurrency

/// The transaction extension identifiers a runtime runs for one extension version.
protocol TransactionExtensionPipelineProviding {
    func transactionExtensions(for chainId: ChainId, extensionVersion: UInt8) async throws -> [String]
}

final class RuntimeTransactionExtensionPipelineProvider: TransactionExtensionPipelineProviding {
    private let chainRegistry: ChainRegistryProtocol

    init(chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry) {
        self.chainRegistry = chainRegistry
    }

    func transactionExtensions(for chainId: ChainId, extensionVersion: UInt8) async throws -> [String] {
        let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chainId)
        let metadata = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute().metadata
        return try metadata.getSignedExtensions(forExtensionVersion: extensionVersion)
    }
}

protocol ExtrinsicVersionProviding {
    func getExtrinsicVersion(for chainId: ChainId, isSigned: Bool) async throws -> Extrinsic.Version
}

/// People and Asset Hub take V5 with the remote-config extension version. A *signed* V5
/// transaction carries its signature in the `VerifyMultiSignature` extension, so on a runtime whose
/// pipeline lacks it the SDK would silently emit an unsigned general transaction; such a runtime still
/// accepts V4, so signed transactions fall back to it. Every other chain is V4.
final class ExtrinsicVersionProvider {
    private let extensionVersionProvider: ExtrinsicExtensionVersionProviding
    private let pipelineProvider: TransactionExtensionPipelineProviding

    init(
        extensionVersionProvider: ExtrinsicExtensionVersionProviding = ExtrinsicExtensionVersionProvider(),
        pipelineProvider: TransactionExtensionPipelineProviding = RuntimeTransactionExtensionPipelineProvider()
    ) {
        self.extensionVersionProvider = extensionVersionProvider
        self.pipelineProvider = pipelineProvider
    }
}

extension ExtrinsicVersionProvider: ExtrinsicVersionProviding {
    func getExtrinsicVersion(for chainId: ChainId, isSigned: Bool) async throws -> Extrinsic.Version {
        guard Self.generalTransactionChains.contains(chainId) else {
            return .V4
        }

        let version = extensionVersionProvider.getExtensionVersion(for: .V5, chainId: chainId)
        guard isSigned, case let .V5(extensionVersion) = version else {
            return version
        }

        let pipeline = try await pipelineProvider.transactionExtensions(
            for: chainId,
            extensionVersion: extensionVersion
        )
        return pipeline.contains(Extrinsic.TransactionExtensionId.verifySignature) ? version : .V4
    }
}

private extension ExtrinsicVersionProvider {
    static var generalTransactionChains: Set<ChainId> {
        [AppConfig.Chains.usernameChain, AppConfig.Chains.assethubChain]
    }
}
