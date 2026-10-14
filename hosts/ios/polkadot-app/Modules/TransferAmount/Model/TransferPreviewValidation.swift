import BigInt
import Coinage

enum TransferPreviewValidation {
    case coinage(TransferPreview)
    /// External payments carry the amount (the preview holds only what is spent) and whether the
    /// plan gives up privacy the user must confirm first.
    case externalPayment(ExternalPaymentPreview, amount: BigUInt, requiresPrivacyConfirmation: Bool)

    var fullAmount: BigUInt {
        switch self {
        case let .coinage(preview): preview.fullAmount
        case let .externalPayment(_, amount, _): amount
        }
    }

    /// Whether the plan spends gaining-privacy funds, so the user must confirm before it is submitted.
    var requiresPrivacyConfirmation: Bool {
        switch self {
        case let .coinage(preview): preview.scope == .withConfirmation
        case let .externalPayment(_, _, requiresPrivacyConfirmation): requiresPrivacyConfirmation
        }
    }
}
