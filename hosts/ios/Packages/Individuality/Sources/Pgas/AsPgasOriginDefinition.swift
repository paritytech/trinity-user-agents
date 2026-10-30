import Foundation
import SubstrateSdk
import ExtrinsicService
import Operation_iOS
import KeyDerivation
import StructuredConcurrency

public struct AsPgasOriginInput {
    let personDeps: PersonProofDependency
    let proofContext: Data
    let day: UInt32
    let proofParams: MembersProofParams

    public init(
        personDeps: PersonProofDependency,
        proofContext: Data,
        day: UInt32,
        proofParams: MembersProofParams
    ) {
        self.personDeps = personDeps
        self.proofContext = proofContext
        self.day = day
        self.proofParams = proofParams
    }
}

public final class AsPgasOriginDefinition: ExtrinsicOriginDefining {
    private let input: AsPgasOriginInput

    public init(input: AsPgasOriginInput) {
        self.input = input
    }

    public func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let resultOperation = ClosureOperation<ExtrinsicOriginDefinitionResponse> { [input] in
            let proofParams = input.proofParams

            let memberKey = try input.personDeps.keyManager.getMemberKey()
            guard proofParams.ringMembers.contains(memberKey) else {
                throw AsResourcesOriginError.memberNotIncluded
            }

            let deps = try dependency()

            let collection: ResourcesPallet.MembershipCollection =
                switch input.personDeps.origin {
                case .lite: .litePeople
                case .full: .people
                }

            let pgasInfo = PGASPallet.AsPgasInfo(
                vrfManager: input.personDeps.keyManager,
                ringIndex: input.personDeps.origin.ringIndex,
                revision: proofParams.revision,
                proofParams: proofParams,
                collection: collection,
                proofContext: input.proofContext,
                day: input.day
            )

            let txExtension = PGASPallet.AsPgasTxExtension(
                extrinsicVersion: extrinsicVersion,
                info: pgasInfo
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

        return CompoundOperationWrapper(targetOperation: resultOperation)
    }
}
