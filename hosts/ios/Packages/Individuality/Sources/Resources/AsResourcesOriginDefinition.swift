import Foundation
import SubstrateSdk
import ExtrinsicService
import Operation_iOS
import KeyDerivation
import StructuredConcurrency

enum AsResourcesOriginError: Error {
    case memberNotIncluded
}

public struct AsResourcesOriginInput {
    public enum Kind {
        case registerStatementStoreAllowance
        case registerNotificationForCollection
        case claimLongTermStorage
    }

    let personDeps: PersonProofDependency
    let proofContext: Data
    let kind: Kind

    public init(
        personDeps: PersonProofDependency,
        proofContext: Data,
        kind: Kind
    ) {
        self.personDeps = personDeps
        self.proofContext = proofContext
        self.kind = kind
    }
}

public final class AsResourcesOriginDefinition: ExtrinsicOriginDefining {
    private let input: AsResourcesOriginInput

    public init(input: AsResourcesOriginInput) {
        self.input = input
    }

    public func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let proofParamsOperation = AsyncTaskOperation { [input] in
            try await input.personDeps
                .proofParamsFetcher
                .fetchParams(blockHash: nil)
        }

        let resultOperation = ClosureOperation<ExtrinsicOriginDefinitionResponse> { [input] in
            let proofParams = try proofParamsOperation.extractNoCancellableResultData()

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

            let allowanceParams = ResourcesPallet.AsRegisterStatementStoreAllowanceParams(
                vrfManager: input.personDeps.keyManager,
                ringIndex: input.personDeps.origin.ringIndex,
                proofParams: proofParams,
                collection: collection,
                proofContext: input.proofContext
            )

            let info: ResourcesPallet.AsResourcesInfo =
                switch input.kind {
                case .registerStatementStoreAllowance:
                    .registerStatementStoreAllowance(allowanceParams)
                case .registerNotificationForCollection:
                    .registerNotificationForCollection(allowanceParams)
                case .claimLongTermStorage:
                    .claimLongTermStorage(
                        ResourcesPallet.AsClaimLongTermStorageParams(
                            vrfManager: input.personDeps.keyManager,
                            ringIndex: input.personDeps.origin.ringIndex,
                            revision: proofParams.revision,
                            proofParams: proofParams,
                            collection: collection,
                            proofContext: input.proofContext
                        )
                    )
                }

            let txExtension = ResourcesPallet.AsResourcesTxExtension(
                extrinsicVersion: extrinsicVersion,
                info: info
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

        resultOperation.addDependency(proofParamsOperation)

        return CompoundOperationWrapper(
            targetOperation: resultOperation,
            dependencies: [proofParamsOperation]
        )
    }
}
