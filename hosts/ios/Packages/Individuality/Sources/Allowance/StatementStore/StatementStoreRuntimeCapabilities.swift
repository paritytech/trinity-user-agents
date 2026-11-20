import Foundation
import SubstrateSdk
import ChainStore
import StructuredConcurrency

public protocol StatementStoreRuntimeCapabilitiesProviding {
    /// Old runtimes take over occupied slots with `set_statement_store_account` under a cooldown instead.
    func supportsSlotReplacement() async throws -> Bool
}

public final class StatementStoreRuntimeCapabilities: StatementStoreRuntimeCapabilitiesProviding {
    private let chainId: ChainId
    private let chainRegistry: ChainResourceProtocol

    public init(chainId: ChainId, chainRegistry: ChainResourceProtocol) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
    }

    public func supportsSlotReplacement() async throws -> Bool {
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        return codingFactory.metadata.getCall(
            from: ResourcesPallet.name,
            with: ResourcesPallet.ReplaceStatementStoreAccountCall.callName
        ) != nil
    }
}
