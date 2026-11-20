import Foundation
import SubstrateSdk
import ExtrinsicService
import Operation_iOS
import KeyDerivation

/// The slot holder signs the transaction payload; the runtime checks it in the `AsResources` extension.
public final class StmtStoreHolderOriginDefinition: ExtrinsicOriginDefining {
    private let signer: any TypedSigningProviding

    public init(signer: any TypedSigningProviding) {
        self.signer = signer
    }

    public func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let operation = ClosureOperation<ExtrinsicOriginDefinitionResponse> { [signer] in
            let deps = try dependency()

            let txExtension = ResourcesPallet.AsResourcesTxExtension(
                extrinsicVersion: extrinsicVersion,
                info: .replaceStatementStoreAccount(
                    ResourcesPallet.AsReplaceStatementStoreAccountParams(signer: signer)
                )
            )

            let builders = deps.builders.map { builder in
                builder.adding(transactionExtension: txExtension)
            }

            return ExtrinsicOriginDefinitionResponse(
                builders: builders,
                senderResolution: deps.senderResolution,
                feePayment: deps.feePayment
            )
        }

        return CompoundOperationWrapper(targetOperation: operation)
    }
}
