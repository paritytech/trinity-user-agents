import Foundation

// Tuning of the motion shine: how bright the streak is, how dark the
// surface falls away from it, the streak's footprint (width across the band,
// length along its diagonal) and where it rests on the gradient axis.
public struct MotionShineParameters {
    let intensity: Double
    let dimming: Double
    let width: Double
    let length: Double
    let center: Double

    public init(
        intensity: Double,
        dimming: Double,
        width: Double,
        length: Double = 0.5,
        center: Double = 0.5
    ) {
        self.intensity = intensity
        self.dimming = dimming
        self.width = width
        self.length = length
        self.center = center
    }
}

public extension MotionShineParameters {
    static let balanceCard = MotionShineParameters(
        intensity: 0.2,
        dimming: 0.7,
        width: 0.18,
        center: 0.35
    )

    static func collectibles(isExpanded: Bool) -> MotionShineParameters {
        MotionShineParameters(
            intensity: 0.08,
            dimming: 0.17,
            width: 0.1,
            center: isExpanded ? 0.5 : 0.25
        )
    }
}
