import Coinage
import Foundation

/// The fungibility ladder: how a recycler's score becomes a bucket, and what a bucket is worth.
///
/// Quantised rather than continuous because the meaning of a difference is not linear, and because
/// discrete steps are what let holdings of the same standing be told apart from ones merely close
/// to each other.
enum CoinageStatusMetrics {
    /// Lowest fungibility percentage in each bucket, most fungible first. Ratio is about 1.53 per
    /// step, so a bucket is roughly a one-and-a-half-fold change in the anonymity set, with the
    /// last two widened because scores that low are rare and not worth separating.
    private static let bucketFloors: [UInt8] = [66, 43, 28, 19, 12, 8, 5, 2, 0]

    /// Buckets run `0` (fully fungible, no bar) to ``maximumBucket`` (no anonymity, full column).
    static var maximumBucket: Int { bucketFloors.count - 1 }

    /// Buckets a fungibility percentage onto the log-ish ladder the bars are drawn in.
    ///
    /// Logarithmic rather than linear because the meaning of a difference is: going from being
    /// fungible with one other coin to four is substantial, going from 510 to 511 is not. Quantised
    /// because discrete lengths are what lets rows with the same standing collapse into one.
    static func bucket(forScore score: UInt8) -> Int {
        let clamped = min(score, CoinageConstants.fullFungibility)

        return bucketFloors.firstIndex { clamped >= $0 } ?? maximumBucket
    }

    /// Penalty for a coin unloaded as one of a batch: the batch links it to the others that came
    /// out with it, which the recycler's own score does not account for.
    ///
    /// A flat step rather than `log(batch size)` because the batch size is not recorded. Two
    /// buckets is about a two-and-a-third-fold linkage, which understates a typical batch; it is a
    /// deliberate approximation, not an estimate.
    static let batchUnloadPenalty = 2
}
