import Foundation
import StatementStore

/// Errors after which delivery stops and the statement is marked failed instead of retried.
public protocol OutgoingStatementTerminalError: Error {}

public protocol StatementSignerResolving<Input>: Sendable {
    associatedtype Input: OutgoingStatementInput

    func currentPeriod() async throws -> UInt32

    func maxStatementSize() async throws -> Int

    /// Signers for `inputs`, given newest first so scarce slots go to the newest statements.
    func resolve(
        _ inputs: [Input],
        period: UInt32
    ) async throws -> [String: Result<StatementStoreSigning, Error>]
}
