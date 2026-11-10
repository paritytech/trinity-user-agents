import Foundation

public struct StatementPublication: Sendable, Equatable {
    public let period: UInt32
    /// True only for the first publication, which runs the post-delivery step.
    public let isFirst: Bool

    public init(period: UInt32, isFirst: Bool) {
        self.period = period
        self.isFirst = isFirst
    }
}

public struct PublishedStatement<Input: OutgoingStatementInput>: Sendable {
    public let input: Input
    public let period: UInt32
    public let createdAt: Date

    public init(input: Input, period: UInt32, createdAt: Date) {
        self.input = input
        self.period = period
        self.createdAt = createdAt
    }
}

public enum OutgoingStatementFailure: Sendable {
    case oversized(size: Int)
    case terminal(Error)
}

/// Keeps pipeline state where each kind already stores it (message and request status, publication period).
public protocol OutgoingStatementStoring<Input>: Sendable {
    associatedtype Input: OutgoingStatementInput

    /// Still waiting for its first publication.
    func isAwaitingFirstDelivery(_ input: Input) async throws -> Bool

    /// Published statements to renew, newest first.
    func renewalCandidates() async throws -> [PublishedStatement<Input>]

    func markPublished(_ input: Input, period: UInt32) async throws -> StatementPublication

    func markFailed(_ input: Input, failure: OutgoingStatementFailure) async throws

    func isAcknowledged(_ input: Input) async throws -> Bool

    func markAcknowledged(_ input: Input) async throws

    func markAbandoned(_ input: Input) async throws
}

public protocol OutgoingStatementPostDelivering<Input>: Sendable {
    associatedtype Input: OutgoingStatementInput

    func didPublishFirstTime(_ input: Input) async
}
