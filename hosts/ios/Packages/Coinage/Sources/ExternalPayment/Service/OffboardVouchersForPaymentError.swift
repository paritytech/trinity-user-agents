import Foundation
import SubstrateSdk

enum OffboardVouchersForPaymentError: Error {
    case emptyVouchers
    case missingRecyclerInfo
    case unexpectedEmptyRevision(RecyclerKey)
    case noSurplusHost(Balance)
    case subscriptionEnded
    case unknownVoucher(CoinageKeyIndex)
}
