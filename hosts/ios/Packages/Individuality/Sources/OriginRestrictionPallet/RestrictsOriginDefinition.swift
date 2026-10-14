import Foundation
import Operation_iOS
import SubstrateSdk
import ExtrinsicService

public final class RestrictsOriginDefinition {
    let enabled: Bool

    public init(enabled: Bool) {
        self.enabled = enabled
    }
}

private extension RestrictsOriginDefinition {
    func createExtensionAppendingWrapper(
        dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        enabled: Bool
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let operation = ClosureOperation<ExtrinsicOriginDefinitionResponse> {
            let dependencies = try dependency()

            let txExtension = OriginRestrictionPallet.TransactionExtension(enabled: enabled)

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

extension RestrictsOriginDefinition: ExtrinsicOriginDefining {
    public func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion _: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        createExtensionAppendingWrapper(dependency: dependency, enabled: enabled)
    }
}
