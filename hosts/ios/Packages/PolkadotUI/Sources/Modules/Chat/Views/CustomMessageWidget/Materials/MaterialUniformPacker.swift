import Foundation

/// The renderer inputs of one frame, as `docs/rfcs/materials.md` defines them.
public struct MaterialFrameInputs: Equatable, Sendable {
    public var time: Float
    public var motion: MaterialMotion
    public var resolution: SIMD2<Float>
    /// 1 for a texture slot that holds 32-bit floats the GPU cannot filter, else 0.
    public var samplingModes: [Int: Int32]

    public init(
        time: Float = 0,
        motion: MaterialMotion = .still,
        resolution: SIMD2<Float>,
        samplingModes: [Int: Int32] = [:]
    ) {
        self.time = time
        self.motion = motion
        self.resolution = resolution
        self.samplingModes = samplingModes
    }
}

/// How the phone is held, as the RFC's device angles in degrees.
public struct MaterialMotion: Equatable, Sendable {
    /// `(gamma / 90, beta / 90)`.
    public var tilt: SIMD2<Float>
    /// `(alpha, beta, gamma)`.
    public var orientation: SIMD3<Float>
    /// Column after column.
    public var rotation: [Float]

    public init(tilt: SIMD2<Float>, orientation: SIMD3<Float>, rotation: [Float]) {
        self.tilt = tilt
        self.orientation = orientation
        self.rotation = rotation
    }

    public static let identity: [Float] = [1, 0, 0, 0, 1, 0, 0, 0, 1]

    /// No sensor, reduced motion, or nothing read yet.
    public static let still = MaterialMotion(tilt: .zero, orientation: .zero, rotation: identity)

    /// The angles of a device-to-world rotation given row after row, `Rz(alpha) Rx(beta) Ry(gamma)`.
    ///
    /// Beta runs the whole turn, from -180 to 180, and alpha from 0 to 360. Gamma stays within 90
    /// degrees, so the sign of the last entry, `cos(beta) cos(gamma)`, says on which side of
    /// vertical beta lies. The same reading as the Android host.
    public init(deviceToWorld rows: [Float]) {
        let upright = rows[8] >= 0
        let sign: Float = upright ? 1 : -1
        let pitch = Self.degrees(asin(min(max(rows[7], -1), 1)))
        let alpha = Self.wrap(Self.degrees(atan2(-sign * rows[1], sign * rows[4])), from: 0)
        let beta = Self.wrap(upright ? pitch : 180 - pitch, from: -180)
        let gamma = Self.degrees(atan2(-sign * rows[6], sign * rows[8]))
        self.init(
            tilt: [gamma / 90, beta / 90],
            orientation: [alpha, beta, gamma],
            rotation: (0 ..< 9).map { rows[($0 % 3) * 3 + $0 / 3] }
        )
    }
}

private extension MaterialMotion {
    static func degrees(_ radians: Float) -> Float {
        radians * 180 / .pi
    }

    static func wrap(_ angle: Float, from start: Float) -> Float {
        let turns = (angle - start).truncatingRemainder(dividingBy: 360)
        return (turns < 0 ? turns + 360 : turns) + start
    }
}

/// Writes a layer's two uniform blocks at the offsets the core laid them out at, std140.
public enum MaterialUniformPacker {
    /// Metal draws into a top-down target.
    static let targetFlip: Float = -1

    public static func pack(
        _ layer: WidgetMaterialLayer,
        inputs: MaterialFrameInputs
    ) -> (vertex: Data, fragment: Data) {
        var vertex = Data(count: layer.blockSizes.vertex)
        var fragment = Data(count: layer.blockSizes.fragment)
        for uniform in layer.uniforms {
            let words = values(of: uniform, layer: layer, inputs: inputs)
            switch uniform.stage {
            case .vertex: write(words, kind: uniform.kind, at: uniform.offset, into: &vertex)
            case .fragment: write(words, kind: uniform.kind, at: uniform.offset, into: &fragment)
            }
        }
        return (vertex, fragment)
    }
}

private extension MaterialUniformPacker {
    static func values(
        of uniform: WidgetMaterialLayer.Uniform,
        layer: WidgetMaterialLayer,
        inputs: MaterialFrameInputs
    ) -> [UInt32] {
        switch uniform.role {
        case .time: [inputs.time.bitPattern]
        case .tilt: [inputs.motion.tilt.x, inputs.motion.tilt.y].map(\.bitPattern)
        case .resolution: [inputs.resolution.x, inputs.resolution.y].map(\.bitPattern)
        case .orientation:
            [inputs.motion.orientation.x, inputs.motion.orientation.y, inputs.motion.orientation.z].map(\.bitPattern)
        case .rotation: inputs.motion.rotation.map(\.bitPattern)
        case .targetFlip: [targetFlip.bitPattern]
        case let .samplingMode(slot): [UInt32(bitPattern: inputs.samplingModes[slot] ?? 0)]
        case let .argument(index):
            if layer.arguments.indices.contains(index), case let .numbers(words) = layer.arguments[index] {
                words
            } else {
                []
            }
        }
    }

    /// A `mat3` is three columns, each padded to 16 bytes.
    static func write(
        _ words: [UInt32],
        kind: WidgetMaterialLayer.Uniform.Kind,
        at offset: Int,
        into block: inout Data
    ) {
        for (position, word) in words.enumerated() {
            let slot = kind == .mat3 ? (position / 3) * 4 + position % 3 : position
            let start = offset + slot * MemoryLayout<UInt32>.size
            guard start + MemoryLayout<UInt32>.size <= block.count else { return }
            withUnsafeBytes(of: word.littleEndian) { bytes in
                block.replaceSubrange(start ..< start + bytes.count, with: bytes)
            }
        }
    }
}
