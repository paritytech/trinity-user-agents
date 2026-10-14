import Foundation
import SubstrateSdk
import Individuality
import ExtrinsicService
import Operation_iOS
import KeyDerivation
import StructuredConcurrency

public struct AsUnloadTokenPeopleInput {
    // all dependencies to generate proof
    let personDeps: PersonProofDependency

    /// Current date for unload token period calculation.
    public let currentDate: Date

    /// Pre-resolved unload token parameters.
    public let resolvedToken: ResolvedUnloadToken

    public init(
        personDeps: PersonProofDependency,
        currentDate: Date,
        resolvedToken: ResolvedUnloadToken
    ) {
        self.personDeps = personDeps
        self.currentDate = currentDate
        self.resolvedToken = resolvedToken
    }
}

/// Origin definition for unload operations using full personhood proof.
///
/// Resolves period/counter from chain state, fetches people and recycler ring members,
/// verifies person's membership in the ring, then defers proof generation to
/// `AsCoinageTxExtension.explicit()`.
public final class AsUnloadTokenPeopleOriginDefinition: ExtrinsicOriginDefining {
    private let input: AsUnloadTokenPeopleInput
    private let voucherKeyManagers: [any BandersnatchKeyManaging]
    private let recyclerRingMemberProvider: any RingProofParamsProviding
    private let blockHash: BlockHashData?

    public init(
        input: AsUnloadTokenPeopleInput,
        voucherKeyManagers: [any BandersnatchKeyManaging],
        recyclerRingMemberProvider: any RingProofParamsProviding,
        blockHash: BlockHashData?
    ) {
        self.input = input
        self.voucherKeyManagers = voucherKeyManagers
        self.recyclerRingMemberProvider = recyclerRingMemberProvider
        self.blockHash = blockHash
    }

    public func createOriginResolutionWrapper(
        for dependency: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
        extrinsicVersion: Extrinsic.Version,
        purpose _: ExtrinsicOriginPurpose
    ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
        let peopleProofParamsOperation = AsyncTaskOperation { [input, blockHash] in
            try await input.personDeps.proofParamsFetcher.fetchParams(blockHash: blockHash)
        }

        let recyclerProofParamsOperation = AsyncTaskOperation { [recyclerRingMemberProvider, blockHash] in
            try await recyclerRingMemberProvider.fetchParams(blockHash: blockHash)
        }

        let resultOperation = ClosureOperation<ExtrinsicOriginDefinitionResponse> { [input, voucherKeyManagers] in
            let peopleProofParams = try peopleProofParamsOperation.extractNoCancellableResultData()
            let recyclerProofParams = try recyclerProofParamsOperation.extractNoCancellableResultData()

            guard !peopleProofParams.ringMembers.isEmpty else {
                throw AsUnloadTokenOriginError.emptyRingKeys
            }

            let memberKey = try input.personDeps.keyManager.getMemberKey()

            guard peopleProofParams.ringMembers.contains(memberKey) else {
                throw AsUnloadTokenOriginError.memberNotIncluded
            }

            let dependencies = try dependency()

            let params = CoinagePallet.AsUnloadTokenPeopleParams(
                keyManager: input.personDeps.keyManager,
                peopleProofParams: peopleProofParams,
                peopleRingIndex: input.personDeps.origin.ringIndex,
                unloadToken: input.resolvedToken,
                voucherKeyManagers: voucherKeyManagers,
                recyclerProofParams: recyclerProofParams
            )

            let txExtension =
                switch input.personDeps.origin {
                case .lite:
                    CoinagePallet.AsCoinageTxExtension(
                        extrinsicVersion: extrinsicVersion,
                        info: .asUnloadTokenLitePeople(params)
                    )
                case .full:
                    CoinagePallet.AsCoinageTxExtension(
                        extrinsicVersion: extrinsicVersion,
                        info: .asUnloadTokenPeople(params)
                    )
                }

            let builders = dependencies.builders.map { builder in
                builder.adding(transactionExtension: txExtension)
            }

            return ExtrinsicOriginDefinitionResponse(
                builders: builders,
                senderResolution: dependencies.senderResolution,
                feePayment: dependencies.feePayment
            )
        }

        resultOperation.addDependency(peopleProofParamsOperation)
        resultOperation.addDependency(recyclerProofParamsOperation)

        return CompoundOperationWrapper(
            targetOperation: resultOperation,
            dependencies: [peopleProofParamsOperation, recyclerProofParamsOperation]
        )
    }
}
