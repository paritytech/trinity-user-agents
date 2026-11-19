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
        /// Not from the export: a local row for the plain set.
        case titanium = 6
    }

    /// Which coins the denominations are drawn as.
    ///
    /// `plain` is what ships: every coin round, unmilled and titanium, telling the denominations
    /// apart by size alone. `varied` is the coinage of four metals, four outlines and milling,
    /// kept for comparison behind the debug flag. Nothing outside that flag can choose it.
    enum CoinSet: Equatable {
        case varied
        case plain

        static let shipped = CoinSet.plain
    }

    /// Our four bands: dull bronze, bright silver, brighter gold, and bimetallic at the top.
    ///
    /// Every band runs the same four outlines in the same order, so a coin's shape names its place
    /// within its metal wherever it sits. The top band has three denominations and so gets the
    /// first three. The reference's own table never pairs a flower with a core, so that mesh is
    /// exported for us specifically; see `NATIVE_ONLY_GEOMETRIES` in its `export-native.mjs`.
    static func geometry(forExponent exponent: Int16, set: CoinSet = .shipped) -> Geometry {
        guard set == .varied else { return .round }

        let clamped = Int(min(max(exponent, 0), CoinageCoinDesign.highestExponent))
        let band = clamped / CoinageCoinDesign.denominationsPerBand

        guard band < CoinageCoinDesign.Band.twin.rawValue else {
            let within = clamped - CoinageCoinDesign.Band.twin.rawValue * CoinageCoinDesign.denominationsPerBand

            return [.roundBimetal, .curvedPolygonBimetal, .flowerBimetal][min(within, 2)]
        }

        let shapes: [Geometry] = [.round, .curvedPolygon, .flower, .scalloped]

        return shapes[clamped % CoinageCoinDesign.denominationsPerBand]
    }

    static func metals(forExponent exponent: Int16, set: CoinSet = .shipped) -> (outer: Metal, core: Metal?) {
        guard set == .varied else { return (.titanium, nil) }

        return switch CoinageCoinDesign.band(forExponent: exponent) {
        case .bronze: (Metal.bronze, nil)
        case .silver: (Metal.silver, nil)
        case .gold: (Metal.gold, nil)
        case .twin: (Metal.gold, Metal.silver)
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
    /// `window` is how far down the field the drawing surface starts: the surface is one screen
    /// tall and pinned to the visible part of a grid, so a coin is drawn that much higher than it
    /// is laid out.
    static func batches(
        for field: CoinageCoinField,
        designs: [CoinageAssetStore.Design],
        window: CGFloat = 0,
        set: CoinSet = .shipped
    ) -> [CoinageMetalRenderer.Batch] {
        var batches: [String: CoinageMetalRenderer.Batch] = [:]
        var order: [String] = []

        for member in field.members {
            let exponent = Int(min(max(member.coin.exponent, 0), CoinageCoinDesign.highestExponent))

            guard let design = designs[safe: exponent] else { continue }

            let geometry = geometry(forExponent: member.coin.exponent, set: set)
            let distance = field.distanceToTarget(member)
            let key = geometry.rawValue

            if batches[key] == nil {
                batches[key] = CoinageMetalRenderer.Batch(geometry: key, instances: [])
                order.append(key)
            }

            batches[key]?.instances.append(
                instance(member, design: design, distance: distance, window: window, set: set)
            )
        }

        return order.compactMap { batches[$0] }
    }

    /// Which coin is under a touch, reading where each one has actually got to rather than where
    /// it is heading. The nearest the viewer wins where two overlap.
    ///
    /// A coin turned edge-on covers a point or two, so the touch is taken against a floor wide
    /// enough to land on: a finger cannot be expected to hit a line.
    static func coin(at point: CGPoint, in field: CoinageCoinField) -> String? {
        field.members
            .filter { contains($0, point) }
            .max { $0.channels.lift.value < $1.channels.lift.value }?
            .coin.id
    }

    /// The least a coin is taken to cover across, so an edge-on one can still be tapped.
    static let touchFloor: CGFloat = 16

    /// Only the face counts: the edge of a turned coin is a few points at most, under the floor.
    private static func contains(_ member: CoinageCoinField.Member, _ point: CGPoint) -> Bool {
        let channels = member.channels
        let half = channels.height.value / 2
        let faceWidth = geometry(forExponent: member.coin.exponent).faceWidth
        let visible = half * faceWidth * abs(cos(channels.turn.value))
        let reach = max(visible, touchFloor / 2)

        return abs(point.x - channels.centreX.value) <= reach
            && abs(point.y - channels.centreY.value) <= half
    }
}

// MARK: - Instances

private extension CoinageScene {
    static func instance(
        _ member: CoinageCoinField.Member,
        design: CoinageAssetStore.Design,
        distance: CGFloat,
        window: CGFloat,
        set: CoinSet
    ) -> CoinageMetalRenderer.Instance {
        let palette = metals(forExponent: member.coin.exponent, set: set)
        let channels = member.channels
        // Luster is the six-tap path, so it fades in only as a coin lands.
        let landing = max(0, 1 - distance / 40)

        return CoinageMetalRenderer.Instance(
            // World y runs up, so screen y is negated here and never in the shader.
            position: SIMD4(
                Float(channels.centreX.value),
                Float(window - channels.centreY.value),
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
                reeds(forExponent: member.coin.exponent, set: set),
                Float(channels.luster.value * landing * landing),
                Float(channels.recede.value),
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
    static func reeds(forExponent exponent: Int16, set: CoinSet) -> Float {
        guard set == .varied, CoinageCoinDesign.design(forExponent: exponent).isReeded else { return 0 }

        return Float((60 + 70 * CoinageCoinDesign.design(forExponent: exponent).size).rounded())
    }
}
