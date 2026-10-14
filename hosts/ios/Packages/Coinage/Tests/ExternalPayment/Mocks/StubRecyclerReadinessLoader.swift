import Foundation
import SubstrateSdk
import Individuality
@testable import Coinage

/// Answers revision 1 for every recycler key; no ring states.
struct StubRecyclerReadinessLoader: RecyclerReadinessLoading {
    func fetchRevisions(for keys: [RecyclerKey], blockHash _: BlockHashData?) async throws -> [RecyclerKey: UInt32] {
        Dictionary(keys.map { ($0, UInt32(1)) }, uniquingKeysWith: { first, _ in first })
    }

    func fetchRecyclerStates(for _: [RecyclerKey]) async throws -> [RecyclerKey: MembersPallet.RingStatus] {
        [:]
    }

    var maxConsolidationValue: UInt32 = 100

    func maxConsolidation() async throws -> UInt32 {
        maxConsolidationValue
    }

    func maxSplitOutputs() async throws -> UInt32 {
        32
    }
}
