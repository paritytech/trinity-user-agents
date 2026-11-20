import Foundation
import PolkadotUI
import Testing

/// Metal reads each uniform at the std140 offset the core laid it out at, so a value written one
/// word off feeds a material the wrong input with no error anywhere.
struct MaterialUniformPackerTests {
    @Test func everyRoleIsWrittenAtItsOffsetInItsStage() {
        // Given
        let uniforms: [WidgetMaterialLayer.Uniform] = [
            .init(stage: .vertex, offset: 0, kind: .float, role: .targetFlip),
            .init(stage: .fragment, offset: 0, kind: .float, role: .time),
            .init(stage: .fragment, offset: 8, kind: .vec2, role: .tilt),
            .init(stage: .fragment, offset: 16, kind: .vec2, role: .resolution),
            .init(stage: .fragment, offset: 32, kind: .vec3, role: .orientation),
            .init(stage: .fragment, offset: 48, kind: .mat3, role: .rotation),
            .init(stage: .fragment, offset: 96, kind: .vec2, role: .argument(0)),
            .init(stage: .fragment, offset: 104, kind: .int, role: .samplingMode(1))
        ]
        let layer = WidgetMaterialLayer(
            material: Data(repeating: 1, count: 32),
            shader: .init(
                vertex: "",
                fragment: "",
                blockSizes: .init(vertex: 16, fragment: 112),
                readsTime: true,
                readsMotion: true
            ),
            uniforms: uniforms,
            textures: [],
            arguments: [.numbers([Float(0.25).bitPattern, Float(-2).bitPattern])]
        )
        let inputs = MaterialFrameInputs(
            time: 1.5,
            motion: MaterialMotion(
                tilt: [0.5, -0.25],
                orientation: [10, 20, 30],
                rotation: [1, 2, 3, 4, 5, 6, 7, 8, 9]
            ),
            resolution: [300, 200],
            samplingModes: [1: 1]
        )

        // When
        let blocks = MaterialUniformPacker.pack(layer, inputs: inputs)

        // Then
        #expect(floats(blocks.vertex) == [-1, 0, 0, 0])
        let padding: Float = 0
        #expect(floats(blocks.fragment) == [
            1.5, padding, 0.5, -0.25,
            300, 200, padding, padding,
            10, 20, 30, padding,
            1, 2, 3, padding,
            4, 5, 6, padding,
            7, 8, 9, padding,
            0.25, -2, Float(bitPattern: 1), padding
        ])
    }

    /// The angles the Android host reads from the same rotations, its `MaterialUniformsTest`, so a
    /// material tilts the same way on both. Beta runs past vertical instead of turning back.
    @Test func anglesFollowTheDevicePastVertical() {
        // Given
        let cos45: Float = 0.70710677
        let flat: [Float] = [1, 0, 0, 0, 1, 0, 0, 0, 1]
        let topRaised45: [Float] = [1, 0, 0, 0, cos45, -cos45, 0, cos45, cos45]
        let topRaised135: [Float] = [1, 0, 0, 0, -cos45, -cos45, 0, cos45, -cos45]
        let rightEdgeDown30: [Float] = [0.8660254, 0, 0.5, 0, 1, 0, -0.5, 0, 0.8660254]
        let quarterTurnLeft: [Float] = [0, -1, 0, 1, 0, 0, 0, 0, 1]

        // When
        let angles = [flat, topRaised45, topRaised135, rightEdgeDown30, quarterTurnLeft].map { rows in
            MaterialMotion(deviceToWorld: rows).orientation.rounded(.toNearestOrAwayFromZero)
        }

        // Then
        #expect(angles == [[0, 0, 0], [0, 45, 0], [0, 135, 0], [0, 0, 30], [90, 0, 0]])
    }

    @Test func tiltIsGammaAndBetaOverNinetyAndRotationGoesColumnByColumn() {
        // Given
        let rows: [Float] = [1, 0, 0, 0, 0.70710677, -0.70710677, 0, 0.70710677, 0.70710677]

        // When
        let motion = MaterialMotion(deviceToWorld: rows)

        // Then
        #expect((motion.tilt * 100).rounded(.toNearestOrAwayFromZero) == [0, 50])
        #expect(motion.rotation == [1, 0, 0, 0, 0.70710677, 0.70710677, 0, -0.70710677, 0.70710677])
    }

    private func floats(_ block: Data) -> [Float] {
        stride(from: 0, to: block.count, by: 4).map { start in
            let word = block[start ..< start + 4].enumerated().reduce(UInt32(0)) { word, byte in
                word | UInt32(byte.element) << (8 * UInt32(byte.offset))
            }
            return Float(bitPattern: word)
        }
    }
}
