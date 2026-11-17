import Foundation
import Metal
import Testing

@testable import polkadot_app

/// The studio cube is baked on the GPU from the lamps in `params.json`. Nothing downstream reports
/// a wrong bake, the coins just reflect the wrong room, so the bake is pinned against a CPU
/// evaluation of the same lamps here.
@Suite("Coin studio bake")
struct CoinageStudioBakeTests {
    private static let faces = 6

    private func device() throws -> MTLDevice {
        try #require(MTLCreateSystemDefaultDevice())
    }

    private func studio() throws -> CoinageStudioBake.Studio {
        let url = try #require(Bundle.main.url(forResource: "params", withExtension: "json"))

        return try JSONDecoder().decode(CoinageStudioBake.Studio.self, from: Data(contentsOf: url))
    }

    private func bake(size: Int, levels: Int) throws -> MTLTexture {
        let device = try device()
        let library = try #require(try? device.makeDefaultLibrary(bundle: .main))

        return try CoinageStudioBake.bake(studio(), device: device, library: library, size: size, levels: levels)
    }

    /// One face of one level, as floats, four per texel.
    private func read(_ texture: MTLTexture, level: Int, face: Int) -> [Float] {
        let side = max(texture.width >> level, 1)
        var pixels = [Float16](repeating: 0, count: side * side * 4)

        pixels.withUnsafeMutableBytes { raw in
            texture.getBytes(
                raw.baseAddress!,
                bytesPerRow: side * 8,
                bytesPerImage: side * side * 8,
                from: MTLRegionMake2D(0, 0, side, side),
                mipmapLevel: level,
                slice: face
            )
        }

        return pixels.map(Float.init)
    }

    /// The reference's radiance, evaluated on the CPU: the nearest lamp met from the front, else
    /// the room gradient.
    private func radiance(_ direction: SIMD3<Float>, in studio: CoinageStudioBake.Studio) -> Float {
        var best = Float.infinity
        var hit: (lamp: CoinageStudioBake.Lamp, u: Float, v: Float)?

        for lamp in studio.lights {
            let normal = SIMD3(lamp.normal[0], lamp.normal[1], lamp.normal[2])
            let center = SIMD3(lamp.center[0], lamp.center[1], lamp.center[2])
            let denom = (direction * normal).sum()

            guard denom < -1e-6 else { continue }

            let t = (center * normal).sum() / denom

            guard t > 0, t < best else { continue }

            let point = direction * t - center
            let u = (point * SIMD3(lamp.uAxis[0], lamp.uAxis[1], lamp.uAxis[2])).sum() / lamp.halfU
            let v = (point * SIMD3(lamp.vAxis[0], lamp.vAxis[1], lamp.vAxis[2])).sum() / lamp.halfV

            guard abs(u) <= 1, abs(v) <= 1 else { continue }

            best = t
            hit = (lamp, u, v)
        }

        if let hit {
            return hit.lamp.tint[0] * hit.lamp.intensity * (1 - 0.12 * (hit.u * hit.u + hit.v * hit.v)) + hit.lamp
                .roomHere
        }

        let blend = min(max((direction.y + 0.35) / 1.2, 0), 1)

        return studio.room.low + (studio.room.high - studio.room.low) * blend * blend * (3 - 2 * blend)
    }

    @Test("The sharp level is the studio: every face texel matches the lamps evaluated on the CPU")
    func sharpLevelMatchesTheLamps() throws {
        let studio = try studio()
        let texture = try bake(size: 64, levels: 3)
        let directions: [(SIMD3<Float>, SIMD3<Float>, SIMD3<Float>)] = [
            (SIMD3(1, 0, 0), SIMD3(0, 0, -1), SIMD3(0, -1, 0)),
            (SIMD3(-1, 0, 0), SIMD3(0, 0, 1), SIMD3(0, -1, 0)),
            (SIMD3(0, 1, 0), SIMD3(1, 0, 0), SIMD3(0, 0, 1)),
            (SIMD3(0, -1, 0), SIMD3(1, 0, 0), SIMD3(0, 0, -1)),
            (SIMD3(0, 0, 1), SIMD3(1, 0, 0), SIMD3(0, -1, 0)),
            (SIMD3(0, 0, -1), SIMD3(-1, 0, 0), SIMD3(0, -1, 0))
        ]
        var worst: Float = 0

        for (face, axes) in directions.enumerated() {
            let pixels = read(texture, level: 0, face: face)

            for row in stride(from: 0, to: 64, by: 7) {
                for column in stride(from: 0, to: 64, by: 7) {
                    // Four sub-texel directions, as the kernel supersamples.
                    var expected: Float = 0
                    for sub in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                        let s = 2 * (Float(column) + Float(sub.0)) / 64 - 1
                        let t = 2 * (Float(row) + Float(sub.1)) / 64 - 1
                        let direction = axes.0 + axes.1 * s + axes.2 * t
                        expected += radiance(direction / (direction * direction).sum().squareRoot(), in: studio) / 4
                    }

                    let baked = pixels[(row * 64 + column) * 4]
                    worst = max(worst, abs(baked - expected))
                }
            }
        }

        // Half floats carry about three decimal digits; a wrong face or a flipped axis is off by
        // the whole brightness of a lamp.
        #expect(worst < 0.01)
    }

    @Test("Each rougher level is smoother than the one before, and keeps the studio's brightness")
    func rougherLevelsAreSmoother() throws {
        let texture = try bake(size: 64, levels: 4)
        var previous: (spread: Float, mean: Float)?

        for level in 0 ..< 4 {
            let pixels = (0 ..< Self.faces).flatMap { read(texture, level: level, face: $0) }
            let reds = stride(from: 0, to: pixels.count, by: 4).map { pixels[$0] }
            let spread = reds.max()! - reds.min()!
            let mean = reds.reduce(0, +) / Float(reds.count)

            if let previous {
                #expect(spread < previous.spread)
                #expect(abs(mean - previous.mean) < previous.mean * 0.25)
            }

            previous = (spread, mean)
        }
    }

    @Test("The sample sequence is the reference's: Hammersley, GGX-distributed, unit length")
    func samplesAreTheReferenceSequence() {
        let samples = CoinageStudioBake.samples(roughness: 0.5)

        #expect(samples.count == 448)
        #expect(samples.allSatisfy { abs(($0 * $0).sum() - 1) < 1e-5 })
        // The first sample has e2 = 0, which GGX puts exactly on the normal.
        #expect(abs(samples[0].z - 1) < 1e-6)
        // The second has e2 = 0.5: cos θ = sqrt(0.5 / (1 + (α⁴ − 1)/2)) for α = 0.25.
        let alpha: Float = 0.25
        let expected = ((1 - 0.5) / (1 + (alpha * alpha - 1) * 0.5)).squareRoot()
        #expect(abs(samples[1].z - expected) < 1e-5)
    }
}
