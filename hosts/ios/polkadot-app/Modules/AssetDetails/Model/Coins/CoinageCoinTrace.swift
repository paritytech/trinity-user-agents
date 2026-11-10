import Foundation

/// What the card says about one holding once it is picked out: what it is worth, whether it is
/// Ready or Clearing, how many hops it has made since it last cleared, and how well it cleared.
///
/// Built once per holdings snapshot and looked up by the coin's id when a coin is tapped, so the
/// coins themselves carry only what the renderer needs. Deliberately no more than four things,
/// and none of them names a mechanism: what the user is told has to stay plain and defensible.
struct CoinageCoinTrace: Equatable {
    /// How well a holding cleared, as the user is told it: the clearing score, nought to a
    /// hundred, in bands that double in width so each grade up is as hard again to reach as the
    /// last, and `unknown` where there is no score at all, which is every coin received from
    /// someone else. Unknown is not a grade: nothing is being claimed.
    ///
    /// Doubling bands rather than even ones, for the same reason the wear is drawn on a doubling
    /// ladder: the first part of a clearing changes most. Cut on the score rather than on the
    /// crowd it stands for, so the grades line up with the percentage wherever it is shown, and
    /// `high` is the top band only, from sixty-five.
    enum Quality: Equatable {
        case unknown
        case veryLow
        case low
        case fair
        case good
        case high

        /// From the clearing score, nought to a hundred.
        init(score: UInt8) {
            switch score {
            case ...8: self = .veryLow
            case ...16: self = .low
            case ...32: self = .fair
            case ...64: self = .good
            default: self = .high
            }
        }
    }

    /// The denomination, formatted the way the card formats every other figure.
    let value: String
    /// Ready or Clearing, exactly as the card labels the block the coin sits in: the user's own
    /// classification, preference and all, so the detail never contradicts the block.
    let partition: CoinageStripLayout.Partition
    /// Hops since the holding last cleared. Nought for one still clearing.
    let hops: Int
    let quality: Quality
}
