import Foundation
import simd

// Kept apart from the loader so the knobs are one short file to open while the look is tuned.
extension CoinageAssetStore {
    /// Material settings of our own, past what the reference exports. They go to the shader after
    /// the exported block, so `params.json` stays the file the export wrote.
    ///
    /// Both exist because the reference is tuned for one large coin under a still light, and here
    /// there are many small ones under a light that moves with the hand. Under that light the
    /// swing in brightness was the only thing the eye could see; the wear and the struck figure
    /// both went under it.
    enum Tuning {
        /// Light added to the whole studio, as if its grey walls were painted a lighter grey. The
        /// room in `params.json` runs from three to eight percent of white against softboxes at
        /// forty and more, and that step is what a coin fell off as it tilted. sRGB twenty percent
        /// is about 0.033 linear and forty about 0.133, so a tenth takes the room from the one to
        /// the other, and the softbox still reads as the highlight. Linear, so it is the same room
        /// at every level of blur.
        static let roomLift: Float = 0.10

        /// A softbox of our own, added to the studio at lookup time. A direction from the coins,
        /// a half angle and a brightness; its edge softens with the blur level, which stands in for
        /// the prefiltering the exported levels had. The key light is 0.68 for scale.
        struct Panel {
            let direction: SIMD3<Float>
            let halfAngle: Float
            let gain: Float
        }

        /// Four panels around the coins: one above, one to each side and one below, so a coin
        /// tipped to any extreme still mirrors a light rather than the dark beyond the studio's
        /// lamps. The shader reads exactly ``panelCount`` of these.
        static let panels = [
            Panel(direction: SIMD3(0, 0.87, 0.5), halfAngle: 22 * .pi / 180, gain: 0.5),
            Panel(direction: SIMD3(-0.9, 0.1, 0.45), halfAngle: 22 * .pi / 180, gain: 0.45),
            Panel(direction: SIMD3(0.9, 0.1, 0.45), halfAngle: 22 * .pi / 180, gain: 0.45),
            Panel(direction: SIMD3(0, -0.87, 0.5), halfAngle: 22 * .pi / 180, gain: 0.4)
        ]

        /// The slot the shader reserves; a studio may fill fewer, and the rest stay dark.
        static let panelCount = 4

        /// How much harder the struck relief slopes than the atlas says. The atlas is baked for a
        /// face hundreds of points across; at sixty the figure flattened into the field.
        static let reliefGain: Float = 1.8
    }
}

extension CoinageAssetStore.Tuning.Panel {
    /// As the shader reads it: unit direction, cosine of the half angle, gain.
    var packed: [Float] {
        let length = max((direction * direction).sum().squareRoot(), 1e-6)

        return [direction.x / length, direction.y / length, direction.z / length, cos(halfAngle), gain]
    }
}
