import Foundation
import ExtrinsicService
import SubstrateSdk
import Operation_iOS

/// An extrinsic origin whose resolution succeeds, echoing the dependency's builders/sender/fee back.
final class StubExtrinsicOrigin: ExtrinsicOriginDefining {
    func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion _: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let operation = ClosureOperation<ExtrinsicOriginDefinitionResponse> {
            let dep = try dependency()
            return ExtrinsicOriginDefinitionResponse(
                builders: dep.builders,
                senderResolution: dep.senderResolution,
                feePayment: dep.feePayment
            )
        }
        return CompoundOperationWrapper(targetOperation: operation)
    }
}
