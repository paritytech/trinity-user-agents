import CoreGraphics
import Foundation

/// Turns the strip layout into the instances the renderer draws, applying our own banding rather
/// than the reference's.
///
/// Geometry only. Which coins exist, which bucket each is in and what order they come in are
/// decided before they get here: both layouts start a new block wherever the partition changes, so
/// they take display order as given rather than sorting it themselves.
///
/// The reference assigns a metal and an outline per denomination; we band four denominations to a
/// metal and run the same four outlines through each band, so its `designs.json` is read only for
/// what belongs to the geometry itself: thickness, face width, relief tile.
enum CoinageScene {
    /// The meshes the reference exports, by what they are.
    enum Geometry: String {
        case round = "g0"
        case flower = "g1"
        case curvedPolygon = "g2"
        case roundBimetal = "g3"
        case polygonBimetal = "g4"
        case curvedPolygonBimetal = "g5"
        case scalloped = "g6"
        case flowerBimetal = "g7"

        /// Face-on extent in coin heights. A property of the outline, not of the value, so it
        /// travels with the mesh rather than with the denomination.
        var faceWidth: CGFloat {
            switch self {
            case .flower,
                 .flowerBimetal: 1.012238
            case .scalloped: 0.96574
            default: 1
            }
        }
    }

    /// Indices into `metals.json`, which the shader reads directly.
    /// Indices into `metals.json`. The reference bands its own denominations differently and uses
    /// all six rows; ours reaches only these three, plus silver as the twin band's core.
    enum Metal: Float {
        case silver = 3
        case gold = 4
        case bronze = 5
    }

    /// Our four bands: dull bronze, bright silver, brighter gold, and bimetallic at the top.
    ///
    /// Every band runs the same four outlines in the same order, so a coin's shape names its place
    /// within its metal wherever it sits. The top band has three denominations and so gets the
    /// first three. The reference's own table never pairs a flower with a core, so that mesh is
    /// exported for us specifically; see `NATIVE_ONLY_GEOMETRIES` in its `export-native.mjs`.
    static func geometry(forExponent exponent: Int16) -> Geometry {
        let clamped = Int(min(max(exponent, 0), CoinageCoinDesign.highestExponent))
        let band = clamped / CoinageCoinDesign.denominationsPerBand

        guard band < CoinageCoinDesign.Band.twin.rawValue else {
            let within = clamped - CoinageCoinDesign.Band.twin.rawValue * CoinageCoinDesign.denominationsPerBand

            return [.roundBimetal, .curvedPolygonBimetal, .flowerBimetal][min(within, 2)]
        }

        let shapes: [Geometry] = [.round, .curvedPolygon, .flower, .scalloped]

        return shapes[clamped % CoinageCoinDesign.denominationsPerBand]
    }

    static func metals(forExponent exponent: Int16) -> (outer: Metal, core: Metal?) {
        switch CoinageCoinDesign.band(forExponent: exponent) {
        case .bronze: (.bronze, nil)
        case .silver: (.silver, nil)
        case .gold: (.gold, nil)
        case .twin: (.gold, .silver)
        }
    }

    /// One holding, as much of it as the renderer and the packing need.
    struct Coin: Equatable {
        let id: String
        let exponent: Int16
        /// `0` untraceable, `1` fully traceable.
        let wear: CGFloat
        let partition: CoinageStripLayout.Partition
        /// How hidden the coin is, on the doubling ladder, which is how the order breaks a tie
        /// between two coins of the same denomination.
        let level: Int
        /// Payments this holding has been through. Each one leaves a pit.
        let hops: Int
    }

    /// Reads the field's live spring values rather than the layout's targets, so what is drawn is
    /// wherever each coin has actually got to.
    static func batches(
        for field: CoinageCoinField,
        designs: [CoinageAssetStore.Design]
    ) -> [CoinageMetalRenderer.Batch] {
        var batches: [String: CoinageMetalRenderer.Batch] = [:]
        var order: [String] = []

        for member in field.members {
            let exponent = Int(min(max(member.coin.exponent, 0), CoinageCoinDesign.highestExponent))

            guard let design = designs[safe: exponent] else { continue }

            let geometry = geometry(forExponent: member.coin.exponent)
            let distance = field.distanceToTarget(member)
            let key = geometry.rawValue

            if batches[key] == nil {
                batches[key] = CoinageMetalRenderer.Batch(geometry: key, instances: [])
                order.append(key)
            }

            batches[key]?.instances.append(instance(member, design: design, distance: distance))
        }

        return order.compactMap { batches[$0] }
    }
}

// MARK: - Instances

private extension CoinageScene {
    static func instance(
        _ member: CoinageCoinField.Member,
        design: CoinageAssetStore.Design,
        distance: CGFloat
    ) -> CoinageMetalRenderer.Instance {
        let palette = metals(forExponent: member.coin.exponent)
        let channels = member.channels
        // Luster is the six-tap path, so it fades in only as a coin lands.
        let landing = max(0, 1 - distance / 40)

        return CoinageMetalRenderer.Instance(
            // World y runs up, so screen y is negated here and never in the shader.
            position: SIMD4(
                Float(channels.centreX.value),
                Float(-channels.centreY.value),
                Float(channels.lift.value),
                Float(channels.height.value)
            ),
            // The strip turns coins by a negative angle; spin about z is for the detail view.
            rotation: SIMD4(
                Float(-channels.turn.value),
                Float(channels.tilt.value),
                0,
                Float(channels.thickness.value)
            ),
            look: SIMD4(
                Float(channels.wear.value),
                palette.outer.rawValue,
                palette.core?.rawValue ?? -1,
                design.tile
            ),
            effects: SIMD4(
                reeds(forExponent: member.coin.exponent),
                Float(channels.luster.value * landing * landing),
                0,
                Float(channels.calm.value)
            ),
            // Streaks follow the animated wear, so a coin visibly cleans up as its ring fills.
            // Pits do not: a payment happened or it did not.
            marks: SIMD4(
                Float(member.coin.hops),
                Float(channels.wear.value),
                member.seed,
                0
            )
        )
    }

    /// Milling is reserved for the round coins above bronze, as a mint reserves it for the coins
    /// worth protecting. The count is what the rim can actually resolve.
    static func reeds(forExponent exponent: Int16) -> Float {
        guard CoinageCoinDesign.design(forExponent: exponent).isReeded else { return 0 }

        return Float((60 + 70 * CoinageCoinDesign.design(forExponent: exponent).size).rounded())
    }
}
