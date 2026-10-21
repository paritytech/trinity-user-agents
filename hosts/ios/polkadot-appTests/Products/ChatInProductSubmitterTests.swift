import Coinage
import DurableTransactions
import Foundation
import Testing
@testable import polkadot_app

struct ChatInProductSubmitterTests {
    /// The native chat sends nothing while the chat product runs person chat, so a payment memo written there would
    /// never reach the recipient: the transfer fails and its transactions are never registered.
    @Test func aPaymentToAContactFailsWithoutCommittingAnything() async throws {
        var committed = false

        await #expect(throws: TransferSubmitterError.chatInProduct) {
            try await ChatInProductSubmitter().sendTransfer(
                TransferMemo(entries: [], totalValue: 0),
                to: Data(repeating: 0xB0, count: 32),
                messageId: "m1"
            ) { _ in committed = true }
        }

        #expect(!committed)
    }
}
