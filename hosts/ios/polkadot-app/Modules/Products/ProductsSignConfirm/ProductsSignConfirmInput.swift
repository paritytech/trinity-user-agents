import TrUAPIHost

/// The signing subset of `UserConfirmationReview`, wrapped for the confirm-only
/// product signing sheet. The wrapped review carries all the raw data the sheet
/// needs to render (call bytes, genesis, raw payload) without any wallet lookup.
enum ProductsSignConfirmInput {
    case signPayload(SignPayloadReview)
    case signRaw(SignRawReview)
    case createTransaction(CreateTransactionReview)

    /// Wraps the signing cases of a `UserConfirmationReview`; `nil` for any
    /// non-signing review.
    init?(review: UserConfirmationReview) {
        switch review {
        case let .signPayload(review):
            self = .signPayload(review)
        case let .signRaw(review):
            self = .signRaw(review)
        case let .createTransaction(review):
            self = .createTransaction(review)
        default:
            return nil
        }
    }
}
