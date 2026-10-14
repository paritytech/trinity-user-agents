import Foundation
import Individuality
import SubstrateSdk

final class MockMembershipStatusChecker: MembershipStatusChecking {
    var statuses: [MembersPallet.RingMember: MembersPallet.RingIndex] = [:]
    private(set) var recordedBlockHash: BlockHashData?

    func checkStatuses(
        of _: [MembershipStatusInput],
        blockHash: BlockHashData?
    ) async throws -> [MembersPallet.RingMember: MembersPallet.RingIndex] {
        recordedBlockHash = blockHash
        return statuses
    }
}
