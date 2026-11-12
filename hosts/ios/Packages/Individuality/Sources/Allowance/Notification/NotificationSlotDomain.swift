import DurableTransactions
import Foundation
import SubstrateSdk
import SubstrateSdkExt

/// Durable ledger keys of notification slot claims.
public enum NotificationSlotDomain {
    public static let domainId = TxDomainId("statement-store-notification-slot")
    public static let policyId = SubmissionPolicyId("statement-store-notification-slot")

    private static let groupPrefix = "notification-slot:"

    /// The engine hands an oracle only the group id, so the claimed account must be recoverable from it.
    public static func groupId(for target: AccountId) -> DurableTxGroupId {
        groupPrefix + target.toHex()
    }

    static func target(of groupId: DurableTxGroupId?) -> AccountId? {
        guard let groupId, groupId.hasPrefix(groupPrefix) else { return nil }

        return try? String(groupId.dropFirst(groupPrefix.count)).fromHex()
    }

    static func policy(for target: AccountId) -> SubmissionPolicy {
        SubmissionPolicy(id: policyId, params: target)
    }
}
