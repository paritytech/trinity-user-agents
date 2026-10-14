import Foundation
import SubstrateSdk
import ExtrinsicService
import Operation_iOS

/// Origin definition for split/transfer operations signed with a coin's keypair.
/// Uses AsCoinage extension with AsCoinageInfo.asCoin variant.
public final class AsCoinOriginDefinition: ExtrinsicOriginDefining {
    public init() {}

    public func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let operation = ClosureOperation<ExtrinsicOriginDefinitionResponse> {
            let dependencies = try dependency()

            let txExtension = CoinagePallet.AsCoinageTxExtension(
                extrinsicVersion: extrinsicVersion,
                info: .asCoin
            )

            let builders = dependencies.builders.map { builder in
                builder.adding(transactionExtension: txExtension)
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
