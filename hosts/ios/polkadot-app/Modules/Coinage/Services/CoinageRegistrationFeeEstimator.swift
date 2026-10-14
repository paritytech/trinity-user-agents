import BigInt
import Coinage
import ExtrinsicService
import Foundation
import SubstrateSdk

/// Fee estimation for the installation registration extrinsic on the contract's chain. The extrinsic
/// format is resolved per estimate from the runtime, so the estimate matches what the durable engine
/// submits (see ``ExtrinsicVersionProvider``).
final class CoinageRegistrationFeeEstimator: RegistrationFeeEstimating, @unchecked Sendable {
    private let chain: ChainProtocol
    private let extrinsicFacade: ExtrinsicSubmissionMonitorFacade
    private let versionProvider: ExtrinsicVersionProviding

    init(
        chain: ChainProtocol,
        extrinsicFacade: ExtrinsicSubmissionMonitorFacade,
        versionProvider: ExtrinsicVersionProviding
    ) {
        self.chain = chain
        self.extrinsicFacade = extrinsicFacade
        self.versionProvider = versionProvider
    }

    func estimateFee(
        _ builder: @escaping ExtrinsicBuilderClosure,
        origin: any ExtrinsicOriginDefining
    ) async throws -> BigUInt {
        let extrinsicVersion = try await versionProvider.getExtrinsicVersion(for: chain.chainId, isSigned: true)
        let operationFactory = try extrinsicFacade.createOperationFactory(
            chain: chain,
            extrinsicVersion: extrinsicVersion
        )

        return try await operationFactory
            .estimateFeeOperation(builder, origin: origin, payingIn: nil)
            .asyncExecute()
            .amount
    }
}
