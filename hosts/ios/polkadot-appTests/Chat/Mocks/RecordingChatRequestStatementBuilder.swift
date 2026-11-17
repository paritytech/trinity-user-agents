@testable import polkadot_app
import Foundation
import os
import StatementDelivery
import StatementStore

final class RecordingChatRequestStatementBuilder: ChatRequestStatementBuilding, @unchecked Sendable {
    private let inputs = OSAllocatedUnfairLock<[OutgoingChatRequestInput]>(initialState: [])
    private let expiries = OSAllocatedUnfairLock<[UInt64]>(initialState: [])

    var builtInputs: [OutgoingChatRequestInput] { inputs.withLock { $0 } }
    var builtExpiries: [UInt64] { expiries.withLock { $0 } }

    func makeUnsignedStatement(for input: OutgoingChatRequestInput) throws -> UnsignedStatement {
        inputs.withLock { $0.append(input) }

        return UnsignedStatement()
    }

    func makeUnsignedStatement(for input: OutgoingChatRequestInput, expiry: UInt64) throws -> UnsignedStatement {
        inputs.withLock { $0.append(input) }
        expiries.withLock { $0.append(expiry) }

        return UnsignedStatement()
    }

    func encodedSize(of _: OutgoingChatRequestInput) throws -> Int {
        0
    }

    func storageTopicFilter(for _: OutgoingChatRequestInput) throws -> TopicFilter {
        .matchAll([])
    }
}
