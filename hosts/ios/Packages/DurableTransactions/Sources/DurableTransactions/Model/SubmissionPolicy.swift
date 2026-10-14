import Foundation

/// Names the ``DurableSubmissionPolicy`` that builds a transaction, and whatever that policy needs to.
///
/// `params` are opaque to the engine and stored exactly as given, so a policy owns their encoding and
/// its evolution. A shape change therefore needs a versioned decoder for the rows already written.
public struct SubmissionPolicy: Sendable, Equatable {
    public let id: SubmissionPolicyId
    public let params: Data

    public init(id: SubmissionPolicyId, params: Data) {
        self.id = id
        self.params = params
    }
}

/// Which ``DurableSubmissionPolicy`` a transaction names, as registered in the policy registry.
public struct SubmissionPolicyId: RawRepresentable, Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public init(_ rawValue: String) {
        self.rawValue = rawValue
    }

    public var description: String { rawValue }
}

/// A transaction waiting for its policy to build it: a ledger row with locks but no attempt.
public struct ScheduledDurableTx: Sendable, Equatable {
    public let id: DurableTxId
    public let domainId: TxDomainId
    public let groupId: DurableTxGroupId?
    public let policy: SubmissionPolicy

    public init(
        id: DurableTxId,
        domainId: TxDomainId,
        groupId: DurableTxGroupId?,
        policy: SubmissionPolicy
    ) {
        self.id = id
        self.domainId = domainId
        self.groupId = groupId
        self.policy = policy
    }
}

/// How an attempt was proven unable to land, which is what decides whether building it again can help.
public enum DurableFailureKind: Sendable, Equatable, CaseIterable {
    /// Never included, and its window has closed. The same effects built again may well land.
    case expired

    /// Included, but its dispatch failed. The same effects built again are likely to fail the same way.
    case dispatchFailed

    /// Refused before it reached a node. May or may not be refused again, depending on why.
    case rejected
}
