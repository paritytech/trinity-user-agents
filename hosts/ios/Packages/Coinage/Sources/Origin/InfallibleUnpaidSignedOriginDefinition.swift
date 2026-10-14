import Foundation
import SubstrateSdk
import ExtrinsicService
import Operation_iOS

/// Origin definition for operations using InfallibleUnpaidSigned mode.
/// No fee payment or ring proof required. Resolves sender and applies nonce internally.
public final class InfallibleUnpaidSignedOriginDefinition: ExtrinsicOriginDefining {
    enum ResolutionError: Error {
        case missingNonce
    }

    public init() {}

    public func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let operation = ClosureOperation {
            let dependencies = try dependency()

            let builders = try dependencies.builders.enumerated().map { _, builder in
                guard let nonce = builder.getNonce() else {
                    throw ResolutionError.missingNonce
                }

                let txExtension = CoinagePallet.AsCoinageTxExtension(
                    extrinsicVersion: extrinsicVersion,
                    info: .infallibleUnpaidSigned(nonce)
                )

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
