import Foundation
import Operation_iOS
import SubstrateSdk
import ExtrinsicService

public enum PeopleLiteOriginDefinitionError: Error {
    case missingNonce
}

public final class PeopleLiteOriginDefinition {
    public init() {}
}

extension PeopleLiteOriginDefinition: ExtrinsicOriginDefining {
    public func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion _: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let operation = ClosureOperation<ExtrinsicOriginDefinitionResponse> {
            let dependencies = try dependency()

            let builders = try dependencies.builders.map { builder in
                guard let nonce = builder.getNonce() else {
                    throw PeopleLiteOriginDefinitionError.missingNonce
                }

                let txExtension = PeopleLitePallet.TransactionExtension(nonce: nonce)

                return builder.adding(transactionExtension: txExtension)
            }

            return ExtrinsicOriginDefinitionResponse(
                builders: builders,
                senderResolution: dependencies.senderResolution,
                feePayment: dependencies.feePayment
            )
        }

        return CompoundOperationWrapper(targetOperation: operation)
    }
}
