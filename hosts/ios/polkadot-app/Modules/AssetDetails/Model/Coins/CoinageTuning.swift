import Foundation

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
        /// How far every reflection is pulled toward the studio's average. Zero is the reference's
        /// full swing between a softbox and the dark beside it; one would be a matte ball. Off
        /// now that the room itself is lifted, which does the same job without smearing the
        /// highlights; kept as a knob while the two are compared.
        static let ambient: Float = 0

        /// Light added to the whole studio, as if its grey walls were painted a lighter grey. The
        /// exported room runs from five to sixteen percent of white against softboxes at seventy
        /// and more, and that seven-to-one step is what a coin fell off as it tilted. Lifting the
        /// room to around forty percent brings the step down to about two to one, and the softbox
        /// still reads as the highlight. Linear, so it is the same room at every level of blur.
        static let roomLift: Float = 0.28

        /// A softbox of our own, added to the studio at lookup time. A direction from the coins,
        /// a half angle and a brightness; its edge softens with the blur level, which stands in for
        /// the prefiltering the exported levels had. The key light is 0.68 for scale.
        struct Panel {
            let direction: SIMD3<Float>
            let halfAngle: Float
            let gain: Float
        }

        /// Three panels, all in front of the coins: one straight ahead over the key light, so the
        /// resting pose is the brightest by a clear step; one high and a fainter one low, so a
        /// coin leaned either way still finds some light. The export's own ceiling strip is as
        /// bright as its key, which is why the front needs the help. The shader reads exactly
        /// ``panelCount`` of these.
        static let panels = [
            Panel(direction: SIMD3(0, 0.15, 1), halfAngle: 28 * .pi / 180, gain: 0.45),
            Panel(direction: SIMD3(0, 0.87, 0.5), halfAngle: 18 * .pi / 180, gain: 0.3),
            Panel(direction: SIMD3(0, -0.87, 0.5), halfAngle: 18 * .pi / 180, gain: 0.2)
        ]

        static let panelCount = 3

        /// How much harder the struck relief slopes than the atlas says. The atlas is baked for a
        /// face hundreds of points across; at sixty the figure flattened into the field.
        static let reliefGain: Float = 1.8
    }
}
