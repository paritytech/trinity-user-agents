import BigInt

struct TransferAmountConfig {
    let prefilledAmountInPlanks: BigUInt?
    let isAmountLocked: Bool
    /// Suppresses the partial-amount option in the degraded-privacy sheet for
    /// flows where the receiver expects exactly the stated amount.
    let requiresExactAmount: Bool
    /// `true` when `recipient.accountId` is a per-payment placeholder rather
    /// than a real on-chain account (e.g. W3S terminal payments). The recipient
    /// row still renders — its `username` carries the human-readable label —
    /// but `saveRecentContact` and the receiver-only validators are skipped
    /// because they'd operate on the meaningless placeholder accountId.
    let recipientIsPlaceholder: Bool

    static let `default` = TransferAmountConfig(
        prefilledAmountInPlanks: nil,
        isAmountLocked: false,
        requiresExactAmount: false,
        recipientIsPlaceholder: false
    )
}
