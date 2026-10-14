import Foundation

/// Per-frame values for the six-petal loading mark. Ported from the dot.li host page loading
/// logo: a highlight walks around the petals once per cycle, dimmed petals never go below a floor.
enum PolkadotLogoLoadingAnimation {
    struct PetalModulation: Equatable {
        let opacity: Double
        let scale: Double
    }

    static let cycle: TimeInterval = 1.4
    static let petalCount = 6
    static let restingOpacity = 0.69
    static let sampleCount = 84

    static func modulation(index: Int, phase: Double) -> PetalModulation {
        var distance = phase - Double(index) / Double(petalCount)
        if distance < 0 { distance += 1 }
        let brightness = max(0.15, pow(max(0, 1 - distance * 2.5), 2))
        return PetalModulation(opacity: brightness, scale: 0.92 + 0.08 * brightness)
    }

    /// Closed loop of samples over one cycle so a repeating keyframe animation joins seamlessly.
    static func keyframes(index: Int) -> (opacity: [Double], scale: [Double]) {
        let samples = (0 ... sampleCount).map { step in
            modulation(index: index, phase: Double(step % sampleCount) / Double(sampleCount))
        }
        return (samples.map(\.opacity), samples.map(\.scale))
    }
}
