import DurableTransactions
import ExtrinsicService
import Foundation
import SubstrateSdk
@testable import Coinage

/// Fixtures and doubles shared by the three rebuild suites.
///
/// Each rebuild is exercised over the parts that decide *what* is rebuilt — the recorded terms, the
/// ledger resolution and the inputs waited on. Building the extrinsic needs a chain and is covered by the
/// harness scenarios instead, so the builders here are only there to construct the value under test.
enum RebuildFixtures {
    static let now = Date(timeIntervalSince1970: 1_700_000_000)

    static func key(_ index: CoinageKeyIndex) -> PublicKey {
        Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
    }

    /// An asset nothing has claimed or handed off.
    static let freeState = CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)

    static func tracked(_ coins: [Coin]) -> [TrackedCoin] {
        coins.map { TrackedCoin(coin: $0, state: freeState) }
    }

    static func coin(_ index: CoinageKeyIndex, exponent: Int16 = 3) -> Coin {
        Coin(
            exponent: exponent,
            derivationIndex: index,
            age: 1,
            isOnchain: true,
            publicKey: key(index)
        )
    }

    static func voucher(_ index: CoinageKeyIndex, inRecycler: Bool = true) -> Voucher {
        Voucher(
            exponent: 3,
            derivationIndex: index,
            allocatedAt: now,
            readyAt: .distantPast,
            remoteState: inRecycler ? .inRecycler(Voucher.Recycler(index: 1, membersCount: 8)) : .unlocated,
            publicKey: key(index)
        )
    }

    /// A scheduled transaction carrying `params`, as the executor would hand it to a policy.
    static func scheduled(id: CoinageTxId = UUID(), policyId: SubmissionPolicyId, params: Data) -> ScheduledDurableTx {
        ScheduledDurableTx(
            id: id,
            domainId: .coinage,
            groupId: nil,
            policy: SubmissionPolicy(id: policyId, params: params)
        )
    }

    /// The ledger row a rebuild reads back: what the transaction consumes and what it mints.
    static func entry(
        id: CoinageTxId,
        inputs: [CoinageTxInput],
        outputs: [OwnAsset]
    ) -> CoinageTxEntry {
        CoinageTxEntry(
            id: id,
            inputs: inputs,
            outputs: outputs,
            txHash: Data(repeating: 0xAB, count: 32),
            checkpoint: BlockRef(number: 100, hash: Data([100])),
            mortality: 60
        )
    }

    static func transferParams(inSeconds: TimeInterval, retryFailures: Bool = true) throws -> Data {
        try CoinageSubmissionParams.splitPolicy(
            TransferSubmissionParams(
                buildUntil: now.addingTimeInterval(inSeconds),
                retryFailures: retryFailures
            )
        ).params
    }

    static func claimParams(inSeconds: TimeInterval, receivedKey: Data) throws -> Data {
        try CoinageSubmissionParams.claimPolicy(
            ClaimSubmissionParams(retryUntil: now.addingTimeInterval(inSeconds), receivedKey: receivedKey)
        ).params
    }
}

/// Never reached by the parts of a rebuild under test; present so the value can be constructed.
struct StubTxFactory: DurableTxMaking {
    func makeExtrinsics(_: [DurableTxRequest], chainId _: ChainId) async throws -> [ExtrinsicBuiltModel] {
        []
    }
}
