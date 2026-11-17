import Foundation
import Metal

/// Bakes a studio into the prefiltered cube the coin shader samples, on the device's GPU.
///
/// The studio is seven or so softbox rectangles and a room gradient, a few hundred bytes. It used
/// to ship baked into fifty-four Radiance files that took megabytes of bundle and, in an
/// unoptimised build, seconds to decode. The GPU evaluates the same description into the same cube
/// in a few milliseconds, with `CoinageStudioBake.metal` a port of the reference's bake script.
enum CoinageStudioBake {
    /// The sharp level's face size and how many levels of blur follow it, which together set the
    /// `envMaxLod` the shader maps roughness onto.
    static let size = 1_024
    static let levels = 9

    /// One softbox as the exporter preprocessed it in `params.json`: a rectangle facing the origin.
    struct Lamp: Decodable, Equatable {
        let center: [Float]
        let uAxis: [Float]
        let vAxis: [Float]
        let normal: [Float]
        let halfU: Float
        let halfV: Float
        let tint: [Float]
        let intensity: Float
        let roomHere: Float

        /// Laid out as `StudioLamp` in the kernel: eighteen floats, no padding.
        var packed: [Float] {
            center + uAxis + vAxis + normal + [halfU, halfV] + tint + [intensity, roomHere]
        }
    }

    struct Room: Decodable, Equatable {
        let low: Float
        let high: Float
    }

    /// A studio: its lamps and the room behind them.
    struct Studio: Decodable, Equatable {
        let lights: [Lamp]
        let room: Room
    }

    enum Failure: Error {
        case deviceRefused(String)
        case missingKernel(String)
    }

    /// How many GGX samples a level of perceptual roughness `roughness` averages, the reference's
    /// own budget: few for the nearly sharp levels, sixteen hundred for the roughest.
    static func sampleCount(roughness: Float) -> Int {
        Int((64 + 1_536 * roughness * roughness).rounded())
    }

    /// The GGX half-vector directions for a level, in tangent space about the normal, drawn from
    /// the Hammersley sequence exactly as the reference draws them.
    static func samples(roughness: Float) -> [SIMD3<Float>] {
        let count = sampleCount(roughness: roughness)
        let alpha = roughness * roughness

        return (0 ..< count).map { index in
            let turn = (Float(index) + 0.5) / Float(count)
            let spread = radicalInverse(UInt32(index))
            let phi = 2 * Float.pi * turn
            let cosTheta = ((1 - spread) / (1 + (alpha * alpha - 1) * spread)).squareRoot()
            let sinTheta = (1 - cosTheta * cosTheta).squareRoot()

            return SIMD3(sinTheta * cos(phi), sinTheta * sin(phi), cosTheta)
        }
    }

    /// Bakes `studio` into a mipped cube, waiting for the GPU to finish.
    static func bake(
        _ studio: Studio,
        device: MTLDevice,
        library: MTLLibrary,
        size: Int = size,
        levels: Int = levels
    ) throws -> MTLTexture {
        let descriptor = MTLTextureDescriptor.textureCubeDescriptor(
            pixelFormat: .rgba16Float,
            size: size,
            mipmapped: true
        )
        descriptor.mipmapLevelCount = levels
        descriptor.usage = [.shaderRead, .shaderWrite]
        descriptor.storageMode = .shared

        guard let texture = device.makeTexture(descriptor: descriptor) else {
            throw Failure.deviceRefused("studio cube")
        }
        guard let queue = device.makeCommandQueue() else {
            throw Failure.deviceRefused("studio bake queue")
        }

        let sharp = try pipeline("studioSharp", device: device, library: library)
        let prefilter = try pipeline("studioPrefilter", device: device, library: library)
        let lamps = try buffer(studio.lights.flatMap(\.packed), device: device)

        // One command buffer per level, each waited for: the sharp level and the first blurred
        // one are the bulk of the work, and the screen's own frames get a turn between them.
        for level in 0 ..< levels {
            let roughness = Float(level) / Float(max(levels - 1, 1))
            let samples = level == 0 ? [] : samples(roughness: roughness)
            var bake = Bake(
                roomLow: studio.room.low,
                roomHigh: studio.room.high,
                lampCount: UInt32(studio.lights.count),
                sampleCount: UInt32(samples.count)
            )
            let directions = try buffer(samples.flatMap { [$0.x, $0.y, $0.z] } + [0, 0, 0], device: device)

            guard let view = texture.makeTextureView(
                pixelFormat: .rgba16Float,
                textureType: .type2DArray,
                levels: level ..< level + 1,
                slices: 0 ..< 6
            ), let commands = queue.makeCommandBuffer(), let encoder = commands.makeComputeCommandEncoder() else {
                throw Failure.deviceRefused("studio level \(level)")
            }

            encoder.setComputePipelineState(level == 0 ? sharp : prefilter)
            encoder.setTexture(view, index: 0)
            encoder.setBuffer(lamps, offset: 0, index: 0)
            encoder.setBytes(&bake, length: MemoryLayout<Bake>.size, index: 1)
            encoder.setBuffer(directions, offset: 0, index: 2)

            // Whole threadgroups rather than `dispatchThreads`: a non-uniform grid needs Apple GPU
            // family 4, which the simulator on an Intel Mac does not have, and it aborted there.
            // The kernels discard threads past the face's edge.
            let side = max(size >> level, 1)
            let group = min(side, 8)
            encoder.dispatchThreadgroups(
                MTLSize(width: (side + group - 1) / group, height: (side + group - 1) / group, depth: 6),
                threadsPerThreadgroup: MTLSize(width: group, height: group, depth: 1)
            )
            encoder.endEncoding()

            commands.commit()
            commands.waitUntilCompleted()

            if let error = commands.error {
                throw Failure.deviceRefused("studio level \(level): \(error.localizedDescription)")
            }
        }

        return texture
    }
}

private extension CoinageStudioBake {
    /// `StudioBake` in the kernel.
    struct Bake {
        var roomLow: Float
        var roomHigh: Float
        var lampCount: UInt32
        var sampleCount: UInt32
    }

    static func buffer(_ floats: [Float], device: MTLDevice) throws -> MTLBuffer {
        guard let buffer = device.makeBuffer(
            bytes: floats,
            length: floats.count * MemoryLayout<Float>.size,
            options: .storageModeShared
        ) else {
            throw Failure.deviceRefused("studio bake buffer")
        }

        return buffer
    }

    static func pipeline(_ name: String, device: MTLDevice, library: MTLLibrary) throws -> MTLComputePipelineState {
        guard let function = library.makeFunction(name: name) else { throw Failure.missingKernel(name) }

        return try device.makeComputePipelineState(function: function)
    }

    /// Van der Corput radical inverse in base two, the second coordinate of the Hammersley set.
    static func radicalInverse(_ index: UInt32) -> Float {
        var bits = index
        bits = (bits << 16) | (bits >> 16)
        bits = ((bits & 0x5555_5555) << 1) | ((bits & 0xAAAA_AAAA) >> 1)
        bits = ((bits & 0x3333_3333) << 2) | ((bits & 0xCCCC_CCCC) >> 2)
        bits = ((bits & 0x0F0F_0F0F) << 4) | ((bits & 0xF0F0_F0F0) >> 4)
        bits = ((bits & 0x00FF_00FF) << 8) | ((bits & 0xFF00_FF00) >> 8)

        return Float(bits) / 4_294_967_296
    }
}
