import Coinage
import Foundation
import Operation_iOS
import SubstrateSdk

struct W3sPaymentRecord: Equatable {
    enum Status: Equatable {
        case pending
        case submitted
        /// Transferred coins confirmed on-chain, awaiting claim.
        case sent
        case claimed
        case failed(reason: String?)
        case revoked
    }

    let paymentId: String
    let recipientTopic: Data
    let merchantName: String?
    let merchantPublicKey: Data
    let amountString: String
    let chainAssetId: String
    let memo: TransferMemo
    /// Finalized block number observed when the statement was submitted.
    let submittedAtBlock: BlockNumber?
    let createdAt: Date
    let updatedAt: Date
    let status: Status
}

extension W3sPaymentRecord {
    func updating(status: Status) -> W3sPaymentRecord {
        W3sPaymentRecord(
            paymentId: paymentId,
            recipientTopic: recipientTopic,
            merchantName: merchantName,
            merchantPublicKey: merchantPublicKey,
            amountString: amountString,
            chainAssetId: chainAssetId,
            memo: memo,
            submittedAtBlock: submittedAtBlock,
            createdAt: createdAt,
            updatedAt: Date(),
            status: status
        )
    }
}

extension W3sPaymentRecord: Operation_iOS.Identifiable {
    var identifier: String { paymentId }
}

extension W3sPaymentRecord.Status {
    /// The W3S payment lifecycle is monotonic once coins are confirmed on-chain.
    /// Guards against concurrent writers (statement submitter vs tracking service)
    /// regressing a more-advanced status — e.g. a late `.submitted`/`.failed` write
    /// clobbering a `.sent`/`.claimed` set by the tracker.
    func canTransition(to next: W3sPaymentRecord.Status) -> Bool {
        switch self {
        case .claimed,
             .revoked:
            // Terminal: never leave.
            false
        case .sent where next == .claimed:
            true
        case .sent where next == .revoked:
            true
        case .sent:
            false
        case .pending,
             .submitted,
             .failed:
            // Pre-confirmation: any transition allowed (lets a window-expiry
            // `.failed` still reconcile forward if coins later confirm).
            true
        }
    }
}
