import CoreGraphics
import Foundation

/// Where every coin is heading, for one of the two arrangements.
///
/// The strip and the grid are the same coins in different places, so this hands the field a set of
/// targets and nothing else changes. A toggle between them is a retarget, which is why the coins
/// fly rather than cut.
enum CoinageArrangement {
    case strip
    case grid

    /// How far in front of the strip the grid sits.
    ///
    /// A coin unturning from edge-on swings half of itself away from the viewer, so a coin that has
    /// started moving used to cut through the neighbours still standing edge-on beside it. Carrying
    /// it forward as it turns keeps it clear of them, and leaves the grid reading as the nearer of
    /// the two arrangements. Comfortably more than the half-diameter a turning coin sweeps back.
    ///
    /// It costs nothing to look at: the projection is orthographic, so depth orders coins without
    /// making the nearer ones larger.
    static let gridLift: CGFloat = 40

    /// One coin picked out of either arrangement by a tap.
    ///
    /// It is drawn large at the left of a band across the top, and the trace of where it came
    /// from sits beside it. The rest of the field moves down under the band and fades back, so the
    /// picked coin is the only bright thing and the others are still there to tap next.
    struct Focus: Equatable {
        let id: String
        /// How tall the band is: never less than the coin, and taller when its trace has more to
        /// say. The caller measures that, because only it knows what the trace holds.
        let band: CGFloat
        /// What part of the field is on screen when the coin is picked: how far down it starts
        /// and how tall it is. The band goes at the bottom of that, over the faded coins, so it
        /// is in reach wherever the page is scrolled; unless there is room below the coins, where
        /// it goes instead so a short grid stays uncovered.
        var window: CGFloat = 0
        var viewport: CGFloat = .infinity
    }

    /// How large a picked coin is drawn: a face the figure can be read on, well past the sixty
    /// points the strip allows and the fifty-two of the roomiest grid.
    static let focusDiameter: CGFloat = 112
    static let focusInset: CGFloat = 8
    /// Clear of the grid, which it flies up out of and back into.
    static let focusLift: CGFloat = gridLift + 60

    /// The least a band can be: the picked coin with an inset above and below.
    static var focusBandMinimum: CGFloat { focusDiameter + focusInset * 2 }

    struct Result {
        let targets: [(coin: CoinageScene.Coin, target: CoinageCoinField.Target)]
        /// What the view needs to be tall enough for.
        let height: CGFloat
        /// Where each partition's header sits, for the labels over the grid.
        let blocks: [CoinageGridLayout.Block]
        /// How far each partition's run reaches across the strip, for the rule under it.
        let runs: [CoinageStripLayout.Span]
        /// Where the band's top sits while a coin is picked, for the trace beside the coin.
        var focusTop: CGFloat?
        /// How tall the coins alone are, before any band below them.
        var coinsHeight: CGFloat = 0
    }

    static func targets(
        for coins: [CoinageScene.Coin],
        arrangement: CoinageArrangement,
        focus: Focus? = nil,
        area: CGSize,
        designs: [CoinageAssetStore.Design],
        stripHeight: CGFloat,
        set: CoinageScene.CoinSet = .shipped
    ) -> Result {
        let laid =
            switch arrangement {
            case .strip: strip(coins, width: area.width, designs: designs, height: stripHeight, set: set)
            case .grid: grid(coins, area: area, designs: designs)
            }

        guard let focus, coins.contains(where: { $0.id == focus.id }) else { return laid }

        return picked(focus, outOf: laid)
    }

    static func design(
        for coin: CoinageScene.Coin,
        in designs: [CoinageAssetStore.Design]
    ) -> CoinageAssetStore.Design? {
        designs[safe: min(max(Int(coin.exponent), 0), designs.count - 1)]
    }
}

// MARK: - Strip

private extension CoinageArrangement {
    static func strip(
        _ coins: [CoinageScene.Coin],
        width: CGFloat,
        designs: [CoinageAssetStore.Design],
        height: CGFloat,
        set: CoinageScene.CoinSet
    ) -> Result {
        var options = CoinageStripLayout.Options()
        options.height = height

        let layout = CoinageStripLayout.layout(
            coins.map { coin in
                CoinageStripLayout.Coin(
                    size: CoinageCoinDesign.design(forExponent: coin.exponent).size,
                    width: CoinageScene.geometry(forExponent: coin.exponent, set: set).faceWidth,
                    thickness: CGFloat(design(for: coin, in: designs)?.thickness ?? 0.075),
                    partition: coin.partition
                )
            },
            width: width,
            options: options
        )

        let targets = zip(coins, layout.coins).map { coin, placement in
            (
                coin,
                CoinageCoinField.Target(
                    centre: placement.centre,
                    height: placement.height,
                    turn: placement.turn,
                    thickness: placement.thickness
                        / CGFloat(design(for: coin, in: designs)?.thickness ?? 0.075),
                    wear: coin.wear,
                    // The strip skips the six-tap luster path; the grid does not.
                    luster: 0,
                    // Not calmed as the reference calms its edge-on coins. Calming swaps the
                    // normal for a plain radial one and fades the reeding, so a turned coin reads
                    // as a band of colour and every coin's rim catches the light alike; the shapes
                    // and the milling are the point of the rim, and they stay.
                    calm: 0
                )
            )
        }

        return Result(
            targets: targets,
            height: height,
            blocks: [],
            runs: layout.partitions
        )
    }
}

// MARK: - Focus

private extension CoinageArrangement {
    /// The arrangement as laid out, with one coin lifted out of it into the band and everything
    /// else faded back where it is.
    ///
    /// The band sits at the bottom of what is on screen, so the picked coin and its trace are in
    /// reach however far down a tall grid the page is, and the other coins stay put to be tapped
    /// next. A grid short enough to leave room below gets the band there instead, under the
    /// coins, so none of them is covered.
    static func picked(_ focus: Focus, outOf laid: Result) -> Result {
        let band = max(focus.band, focusBandMinimum)
        let top = bandTop(for: focus, coinsHeight: laid.height)
        let targets = laid.targets.map { entry -> (coin: CoinageScene.Coin, target: CoinageCoinField.Target) in
            var target = entry.target

            if entry.coin.id == focus.id {
                target.centre = pickedCentre(bandTop: top)
                target.height = focusDiameter
                target.turn = 0
                target.tilt = 0
                target.thickness = 1
                target.luster = 1
                target.calm = 0
                target.lift = focusLift
                target.recede = 0
            } else {
                target.recede = 1
            }

            return (entry.coin, target)
        }

        return Result(
            targets: targets,
            height: max(laid.height, top + band),
            blocks: laid.blocks,
            runs: laid.runs,
            focusTop: top,
            coinsHeight: laid.height
        )
    }
}

extension CoinageArrangement {
    /// Where the band's top goes for what is on screen now: at the bottom of it, or below the
    /// coins when they leave room there. Asked again as the page scrolls, so the band stays on
    /// screen rather than where it was picked.
    static func bandTop(for focus: Focus, coinsHeight: CGFloat) -> CGFloat {
        let band = max(focus.band, focusBandMinimum)
        let visibleBottom = focus.window + focus.viewport - focusInset
        let roomBelow = coinsHeight + band <= visibleBottom

        return roomBelow ? coinsHeight : max(visibleBottom - band, focus.window)
    }

    /// Where the picked coin sits in a band whose top is here.
    static func pickedCentre(bandTop: CGFloat) -> CGPoint {
        CGPoint(x: focusInset + focusDiameter / 2, y: bandTop + focusBandMinimum / 2)
    }
}

// MARK: - Grid

private extension CoinageArrangement {
    static func grid(
        _ coins: [CoinageScene.Coin],
        area: CGSize,
        designs _: [CoinageAssetStore.Design]
    ) -> Result {
        let byId = Dictionary(coins.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let layout = CoinageGridLayout.layout(
            coins.map {
                CoinageGridLayout.Item(
                    id: $0.id,
                    exponent: $0.exponent,
                    partition: $0.partition
                )
            },
            area: area
        )

        let targets = layout.cells.compactMap { cell -> (CoinageScene.Coin, CoinageCoinField.Target)? in
            guard let coin = byId[cell.id] else { return nil }

            let height = layout.diameter * CoinageCoinDesign.design(forExponent: coin.exponent).size

            return (coin, single(coin, centre: cell.centre, height: height))
        }

        return Result(
            targets: targets,
            height: layout.height,
            blocks: layout.blocks,
            // The grid labels its blocks with headers of its own.
            runs: []
        )
    }

    static func single(
        _ coin: CoinageScene.Coin,
        centre: CGPoint,
        height: CGFloat
    ) -> CoinageCoinField.Target {
        CoinageCoinField.Target(
            centre: centre,
            height: height,
            turn: 0,
            thickness: 1,
            wear: coin.wear,
            luster: 1,
            calm: 0,
            lift: CoinageArrangement.gridLift
        )
    }
}
