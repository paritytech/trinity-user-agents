import Coinage
import DurableTransactions
import Foundation
import Testing

@testable import polkadot_app

/// The coin link alone cannot tell a reclaim from an own spend; the stored kind must survive the round trip.
@Suite("Coinage reclaim ledger")
struct CoinageReclaimLedgerTests {
    enum InputKind: CaseIterable {
        case own
        case reclaimed
    }

    @Test("a coin input reads back with the kind it was registered as", arguments: InputKind.allCases)
    func inputKindRoundTrips(kind: InputKind) async throws {
        let world = try await CoinageLedgerWorld()
        let input: CoinageTxInput
        switch kind {
        case .own:
            input = world.coin(1).asInput
        case .reclaimed:
            try await world.precommit([world.coin(1)])
            input = .coin(.reclaimed(CoinageLedgerWorld.keyIndex(1), CoinageLedgerWorld.coinKey(1)))
        }

        try await world.ledger.register([registration(inputs: [input], groupId: world.messageId)])

        let entry = try #require(try await world.scheduledEntries().first)
        #expect(entry.inputs == [input])
    }
}

private extension CoinageReclaimLedgerTests {
    func registration(inputs: [CoinageTxInput], groupId: CoinageTxGroupId) -> CoinageTxRegistration {
        CoinageTxRegistration(
            txHash: Data(repeating: 0x01, count: 32),
            checkpoint: BlockRef(number: 100, hash: Data([100])),
            mortalityBlocks: 64,
            groupId: groupId,
            inputs: inputs,
            outputs: []
        )
    }
}
