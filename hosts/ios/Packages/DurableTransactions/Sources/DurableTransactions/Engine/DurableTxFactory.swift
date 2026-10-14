import ExtrinsicService
import Foundation
import SDKLogger
import SubstrateSdk

/// Builds and signs declared transactions for one chain.
///
/// The one place an extrinsic is produced, shared by registration and by every submission policy that
/// builds a transaction again later — so grouping, ordering and mortality handling are the same
/// wherever bytes come from, rather than each caller reaching for the chain tools itself.
public protocol DurableTxMaking: Sendable {
    func makeExtrinsics(
        _ requests: [DurableTxRequest],
        chainId: ChainId
    ) async throws -> [ExtrinsicBuiltModel]
}

public struct DurableTxFactory: DurableTxMaking {
    private let chainTools: any DurableChainToolsProviding
    private let logger: SDKLoggerProtocol?

    public init(chainTools: any DurableChainToolsProviding, logger: SDKLoggerProtocol?) {
        self.chainTools = chainTools
        self.logger = logger
    }

    public func makeExtrinsics(
        _ requests: [DurableTxRequest],
        chainId: ChainId
    ) async throws -> [ExtrinsicBuiltModel] {
        guard !requests.isEmpty else { return [] }

        let operationFactory = try await chainTools.extrinsicOperationFactory(for: chainId)

        return try await ExtrinsicBatchBuilder(operationFactory: operationFactory, logger: logger)
            .build(requests)
    }
}
