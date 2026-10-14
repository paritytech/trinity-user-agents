import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk

public protocol RingProofParamsProviding {
    func fetchParams(blockHash: BlockHashData?) async throws -> MembersProofParams
}

public protocol RingProofParamsProviderMaking {
    func createProvider(for ringIndex: UInt32) -> any RingProofParamsProviding
}

public final class RingProofParamsProviderFactory: RingProofParamsProviderMaking {
    let proofParamsFetcher: MembershipProofParamsFetching
    let collectionIdentifier: MembersPallet.CollectionIdentifier

    public init(
        collectionIdentifier: MembersPallet.CollectionIdentifier,
        proofParamsFetcher: MembershipProofParamsFetching,
    ) {
        self.collectionIdentifier = collectionIdentifier
        self.proofParamsFetcher = proofParamsFetcher
    }

    public func createProvider(for ringIndex: UInt32) -> any RingProofParamsProviding {
        RingProofParamsProvider(
            collectionIdentifier: collectionIdentifier,
            ringIndex: ringIndex,
            proofParamsFetcher: proofParamsFetcher
        )
    }
}

/// Provides proof params for a specific ring index.
private final class RingProofParamsProvider: RingProofParamsProviding {
    private let proofParamsFetcher: MembershipProofParamsFetching
    private let collectionIdentifier: MembersPallet.CollectionIdentifier
    private let ringIndex: MembersPallet.RingIndex

    init(
        collectionIdentifier: MembersPallet.CollectionIdentifier,
        ringIndex: MembersPallet.RingIndex,
        proofParamsFetcher: MembershipProofParamsFetching
    ) {
        self.collectionIdentifier = collectionIdentifier
        self.ringIndex = ringIndex
        self.proofParamsFetcher = proofParamsFetcher
    }

    func fetchParams(blockHash: BlockHashData?) async throws -> MembersProofParams {
        try await proofParamsFetcher.fetchOrError(
            for: ringIndex,
            collectionId: collectionIdentifier,
            blockHash: blockHash
        )
    }
}
