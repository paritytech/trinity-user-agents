import CoreGraphics
import Foundation

/// The summary strip: every coin at a fixed height, in a fixed width.
///
/// Ported line for line from `src/layout/strip.js` in the `coinage-viz` reference and checked
/// against its conformance vectors, so the two stay the same strip.
///
/// As coins are added the strip gives ground in a fixed order. First the margins close, from ten
/// points toward four, which is why ten coins keep their full margin and nothing jumps at the coin
/// that first does not fit. Then the coins turn about their vertical axis, the face shrinking as
/// cosine and the edge growing as sine, solved exactly for the width rather than stepped. Fully
/// edge-on they get thinner, which is how a thousand coins fit in a thousand points at one point
/// each.
enum CoinageStripLayout {
    struct Options: Equatable {
        var height: CGFloat = 60
        var margin: CGFloat = 10
        var minimumMargin: CGFloat = 4
        /// Between the Clearing and Ready runs, face on. Twice a margin and then some: it is the
        /// only thing separating the two, now that the bar above them is gone.
        var gap: CGFloat = 26
        var minimumGap: CGFloat = 8

        init() {}
    }

    /// One coin's design, as the strip needs it. All three are in coin heights.
    struct Coin: Equatable {
        /// Diameter as a fraction of the strip height.
        let size: CGFloat
        /// Face-on extent.
        let width: CGFloat
        let thickness: CGFloat
        let partition: Partition
    }

    enum Partition: String, Equatable {
        case clearing
        case ready
    }

    struct Placement: Equatable {
        let centre: CGPoint
        let height: CGFloat
        let turn: CGFloat
        let thickness: CGFloat
        /// What the coin covers horizontally once turned.
        let visible: CGFloat
    }

    /// A run of one partition, for the label under it.
    struct Span: Equatable {
        let partition: Partition
        let start: CGFloat
        var end: CGFloat
        var count: Int
    }

    struct Layout: Equatable {
        let turn: CGFloat
        let margin: CGFloat
        let gap: CGFloat
        /// How far coins had to be thinned past 90 degrees. `1` is their own thickness.
        let thinning: CGFloat
        let coins: [Placement]
        let partitions: [Span]
        /// Total width taken, which is what the strip actually fills.
        let used: CGFloat
    }

    static func layout(_ coins: [Coin], width: CGFloat, options: Options = Options()) -> Layout {
        guard !coins.isEmpty else {
            return Layout(
                turn: 0,
                margin: options.margin,
                gap: options.gap,
                thinning: 1,
                coins: [],
                partitions: [],
                used: 0
            )
        }

        let totals = totals(of: coins, options: options)
        let solved = solve(totals, width: width, options: options)

        return place(coins, solved: solved, options: options)
    }

    /// How calm the shading of a nearly edge-on coin should be, so it reads as a clean band of
    /// colour rather than as a lit disc seen from the side.
    static func edgeCalm(turn: CGFloat) -> CGFloat {
        let progress = min(max((abs(turn) - 60 * .pi / 180) / (25 * .pi / 180), 0), 1)

        return progress * progress * (3 - 2 * progress)
    }
}

// MARK: - Solving

private extension CoinageStripLayout {
    struct Totals {
        let faces: CGFloat
        let edges: CGFloat
        let margins: Int
        let boundaries: Int
    }

    struct Solution {
        let turn: CGFloat
        let margin: CGFloat
        let gap: CGFloat
        let thinning: CGFloat
    }

    static func totals(of coins: [Coin], options: Options) -> Totals {
        let boundaries = zip(coins, coins.dropFirst()).count { $0.partition != $1.partition }

        return Totals(
            faces: coins.reduce(0) { $0 + options.height * $1.size * $1.width },
            edges: coins.reduce(0) { $0 + options.height * $1.size * $1.thickness },
            margins: coins.count - 1 - boundaries,
            boundaries: boundaries
        )
    }

    static func solve(_ totals: Totals, width: CGFloat, options: Options) -> Solution {
        let faceOn = totals.faces
            + CGFloat(totals.margins) * options.margin
            + CGFloat(totals.boundaries) * options.gap

        guard faceOn > width else {
            return Solution(turn: 0, margin: options.margin, gap: options.gap, thinning: 1)
        }

        let room = width - totals.faces
        let full = CGFloat(totals.margins) * options.margin + CGFloat(totals.boundaries) * options.gap
        let least = CGFloat(totals.margins) * options.minimumMargin
            + CGFloat(totals.boundaries) * options.gap

        if room >= least, full > 0 {
            let squeezed = totals.margins > 0
                ? (room - CGFloat(totals.boundaries) * options.gap) / CGFloat(totals.margins)
                : 0

            return Solution(
                turn: 0,
                margin: max(options.minimumMargin, squeezed),
                gap: options.gap,
                thinning: 1
            )
        }

        return turned(totals, width: width, options: options)
    }

    /// `A·cos θ + B·sin θ = C`, where A is everything that shrinks as the coins turn and B is the
    /// edge that grows in its place. Past the point where even a wall of edges is too wide, the
    /// angle stops at 90 degrees and the coins thin instead.
    static func turned(_ totals: Totals, width: CGFloat, options: Options) -> Solution {
        let shrinking = totals.faces
            + CGFloat(totals.margins) * options.minimumMargin
            + CGFloat(totals.boundaries) * (options.gap - options.minimumGap)
        let growing = totals.edges
        let available = width - CGFloat(totals.boundaries) * options.minimumGap
        let radius = (shrinking * shrinking + growing * growing).squareRoot()

        let turn: CGFloat
        let thinning: CGFloat

        if growing >= available {
            turn = .pi / 2
            thinning = available / growing
        } else {
            turn = min(.pi / 2, atan2(growing, shrinking) + acos(min(1, available / radius)))
            thinning = 1
        }

        return Solution(
            turn: turn,
            margin: options.minimumMargin * cos(turn),
            gap: options.minimumGap + (options.gap - options.minimumGap) * cos(turn),
            thinning: thinning
        )
    }
}

// MARK: - Placing

private extension CoinageStripLayout {
    static func place(_ coins: [Coin], solved: Solution, options: Options) -> Layout {
        let cosine = cos(solved.turn)
        let sine = sin(solved.turn)

        var placements: [Placement] = []
        var partitions: [Span] = []
        var cursor: CGFloat = 0

        for (index, coin) in coins.enumerated() {
            if index > 0 {
                cursor += coins[index - 1].partition != coin.partition ? solved.gap : solved.margin
            }

            let height = options.height * coin.size
            let thickness = coin.thickness * solved.thinning
            let visible = height * (coin.width * cosine + thickness * sine)

            placements.append(
                Placement(
                    centre: CGPoint(x: cursor + visible / 2, y: options.height / 2),
                    height: height,
                    turn: solved.turn,
                    thickness: thickness,
                    visible: visible
                )
            )

            if var last = partitions.last, last.partition == coin.partition {
                last.end = cursor + visible
                last.count += 1
                partitions[partitions.count - 1] = last
            } else {
                partitions.append(
                    Span(partition: coin.partition, start: cursor, end: cursor + visible, count: 1)
                )
            }

            cursor += visible
        }

        return Layout(
            turn: solved.turn,
            margin: solved.margin,
            gap: solved.gap,
            thinning: solved.thinning,
            coins: placements,
            partitions: partitions,
            used: cursor
        )
    }
}
