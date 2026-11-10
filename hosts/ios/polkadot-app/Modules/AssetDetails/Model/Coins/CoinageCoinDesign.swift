import CoreGraphics

/// What a denomination looks like as a coin: its metal, its outline and its size.
///
/// Banded the way a circulating coinage is. Four consecutive denominations share a metal and run
/// through the same four outlines, so the metal tells you roughly what a coin is worth and the
/// outline tells you which of the four it is. Size climbs gently across the whole range, enough to
/// separate neighbours without turning the field into a bar chart.
enum CoinageCoinDesign {
    /// Outline family. Every one of these exists on a real coin somewhere.
    enum Shape: Equatable {
        case round
        /// Equilateral curved polygon of constant width, as on the UK 20p and 50p.
        case curvedPolygon(sides: Int, rounding: CGFloat)
        /// Spain's 20 centimos: a round edge with smooth indents.
        case flower(notches: Int, depth: CGFloat, width: CGFloat, rounding: CGFloat)
        /// Convex lobes meeting in rounded cusps.
        case scalloped(lobes: Int, lobe: CGFloat, rounding: CGFloat)
    }

    struct Design: Equatable {
        let shape: Shape
        /// Diameter as a fraction of the largest denomination's.
        let size: CGFloat
        /// Whether the edge is milled.
        let isReeded: Bool
    }

    /// The four metals, lowest band first. The top band is bimetallic, which is where a real
    /// coinage puts its largest pieces too.
    enum Band: Int, CaseIterable {
        case bronze = 0
        case silver = 1
        case gold = 2
        case twin = 3
    }

    /// Exponents `0...14`, four to a band. The top band is one short, which is the chain's doing.
    static let denominationsPerBand = 4
    static let highestExponent: Int16 = 14

    static func band(forExponent exponent: Int16) -> Band {
        let clamped = Int(min(max(exponent, 0), highestExponent))

        return Band(rawValue: min(clamped / denominationsPerBand, Band.allCases.count - 1)) ?? .bronze
    }

    static func design(forExponent exponent: Int16) -> Design {
        let clamped = Int(min(max(exponent, 0), highestExponent))
        let band = band(forExponent: exponent)
        let shape = shapes[clamped % denominationsPerBand]

        return Design(
            shape: shape,
            size: size(forExponent: clamped),
            // Milling is what a mint adds to coins worth protecting, so the coppers go without.
            isReeded: band != .bronze && shape == .round
        )
    }

    /// The same four outlines in every band, so a coin's shape names its place within its metal.
    private static let shapes: [Shape] = [
        .round,
        .curvedPolygon(sides: 7, rounding: 0.035),
        .flower(notches: 7, depth: 0.04, width: 0.15, rounding: 0.055),
        .scalloped(lobes: 10, lobe: 0.15, rounding: 0.012)
    ]

    private static func size(forExponent exponent: Int) -> CGFloat {
        let progress = CGFloat(exponent) / CGFloat(highestExponent)

        return 0.82 + 0.18 * progress
    }
}

/// How traceable a coin still is, as the reference renderer measures it.
///
/// A level counts doublings of the crowd a coin hides in: level 0 is alone, level `n` means fewer
/// than `2^n` others. The difference between 510 others and 511 is nothing; between one and four it
/// is most of the story, so the early steps move fastest.
enum CoinageWear {
    /// A recycler ring holds 767 keys, so a coin can be fungible with at most 766 others.
    static let ringCapacity = 767
    static let maximumLevel = level(hiddenAmong: ringCapacity - 1)

    static func level(hiddenAmong others: Int) -> Int {
        others <= 0 ? 0 : Int(log2(Double(others))) + 1
    }

    static func amount(forLevel level: Int) -> CGFloat {
        let progress = min(max(CGFloat(level) / CGFloat(maximumLevel), 0), 1)

        return pow(1 - progress, 1.15)
    }

    /// The level a wear amount came from, for grouping. `amount` is monotone in level, so this is
    /// the nearest level rather than an inverse that could miss by rounding.
    static func level(forAmount wear: CGFloat) -> Int {
        (0 ... maximumLevel).min { abs(amount(forLevel: $0) - wear) < abs(amount(forLevel: $1) - wear) } ?? 0
    }

    /// Wear for a holding whose recycler we have no record of, which is every coin received from a
    /// peer. Nothing can be credited, so it wears as though it hides among nobody.
    static let unknown = amount(forLevel: 0)
}
