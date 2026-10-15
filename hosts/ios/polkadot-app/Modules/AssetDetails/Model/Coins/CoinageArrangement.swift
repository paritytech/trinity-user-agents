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

    struct Result {
        let targets: [(coin: CoinageScene.Coin, target: CoinageCoinField.Target)]
        /// What the view needs to be tall enough for.
        let height: CGFloat
        /// Where each partition's header sits, for the labels over the grid.
        let blocks: [CoinageGridLayout.Block]
        /// How far each partition's run reaches across the strip, for the rule under it.
        let runs: [CoinageStripLayout.Span]
    }

    static func targets(
        for coins: [CoinageScene.Coin],
        arrangement: CoinageArrangement,
        area: CGSize,
        designs: [CoinageAssetStore.Design],
        stripHeight: CGFloat
    ) -> Result {
        switch arrangement {
        case .strip: strip(coins, width: area.width, designs: designs, height: stripHeight)
        case .grid: grid(coins, area: area, designs: designs)
        }
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
        height: CGFloat
    ) -> Result {
        var options = CoinageStripLayout.Options()
        options.height = height

        let layout = CoinageStripLayout.layout(
            coins.map { coin in
                CoinageStripLayout.Coin(
                    size: CoinageCoinDesign.design(forExponent: coin.exponent).size,
                    width: CoinageScene.geometry(forExponent: coin.exponent).faceWidth,
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
                    calm: CoinageStripLayout.edgeCalm(turn: placement.turn)
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
