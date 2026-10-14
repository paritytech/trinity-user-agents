import Foundation
import Individuality
import SubstrateSdk

final class MockMembershipProofParamsFetcher: MembershipProofParamsFetching {
    var params: MembersProofParams? = MembersProofParams(
        ringMembers: [],
        ringSize: .domain11,
        revision: 0
    )
    var revision: MembersPallet.RevisionIndex? = 3
    private(set) var recordedFetchBlockHash: BlockHashData?
    private(set) var recordedRevisionBlockHash: BlockHashData?

    func fetch(
        for _: MembersPallet.RingIndex,
        collectionId _: MembersPallet.CollectionIdentifier,
        blockHash: BlockHashData?
    ) async throws -> MembersProofParams? {
        recordedFetchBlockHash = blockHash
        return params
    }

    func fetchCurrentRevision(
        for _: MembersPallet.RingIndex,
        collectionId _: MembersPallet.CollectionIdentifier,
        blockHash: BlockHashData?
    ) async throws -> MembersPallet.RevisionIndex? {
        recordedRevisionBlockHash = blockHash
        return revision
    }
}
