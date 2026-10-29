import Foundation
import StatementStore

/// One statement that must stay in the statement store until the peer reacts to it.
public protocol OutgoingStatementInput: Sendable {
    var statementId: String { get }

    /// Signs the copy when no notification slot is free and the user consented.
    var identitySignKeyId: String { get }

    /// The one-shot account funded for `period`; re-derived instead of stored.
    func oneShotKeyId(period: UInt32) throws -> String
}

/// Builds the statement for a kind: payload, topics, channel, expiry. Signing happens later.
public protocol OutgoingStatementBuilding<Input>: Sendable {
    associatedtype Input: OutgoingStatementInput

    func makeUnsignedStatement(for input: Input) throws -> UnsignedStatement

    func encodedSize(of input: Input) throws -> Int

    /// Where published copies of the input live, to check whether one is still stored.
    func storageTopicFilter(for input: Input) throws -> TopicFilter
}
