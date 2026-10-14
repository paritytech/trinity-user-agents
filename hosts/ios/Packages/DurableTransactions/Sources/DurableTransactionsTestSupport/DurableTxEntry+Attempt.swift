import DurableTransactions
import Foundation

public extension DurableTxEntry {
    /// The attempt a submitted row always carries.
    ///
    /// Test code deals in rows that were registered with an extrinsic, so a missing attempt is a
    /// broken fixture rather than a condition under test — and failing loudly here is what keeps a
    /// scheduled row from silently being judged against placeholder bytes.
    var submittedAttempt: DurableTxAttempt {
        guard let attempt else {
            preconditionFailure("entry \(id) has no attempt: it was scheduled, never submitted")
        }

        return attempt
    }
}
