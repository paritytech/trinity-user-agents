import Foundation
import Metal
@testable import PolkadotUI
import SwiftUI
import Testing
@testable import polkadot_app

/// The core writes the Metal source and Swift binds it by entry point and buffer index, so nothing
/// but a GPU run shows the two agree. The fixture, `DebugMaterialLayer.json`, is one layer of a
/// material that mixes its base half and half with blue at the top and red at the bottom, so the
/// base still decides each row.
///
/// Its two shader sources are the core's own output, the files
/// `truapi-host materials resolve <face.json> --materials <dir> --metal --out <out>` writes as
/// `node0-layer0.vertex.metal` and `node0-layer0.fragment.metal`. The rest is that run's
/// `node0-layer0.txt`, the block sizes, flags, uniforms and textures, copied by hand into the keys
/// Swift's synthesized `Decodable` reads, with the material ID in base64.
@MainActor
struct MaterialShaderCompilerTests {
    @Test func theCoreWrittenShaderCompilesToAPipeline() async throws {
        // Given
        let gpu = try #require(MaterialGPU.shared)
        let layer = try DebugMaterialsCardViewModel.bundledLayer()

        // When
        let pipeline = try await MaterialShaderCompiler.compile(layer, device: gpu.device, pixelFormat: .bgra8Unorm)

        // Then
        #expect(pipeline.device === gpu.device)
    }

    @Test func aBrokenFragmentThrows() async throws {
        // Given
        let gpu = try #require(MaterialGPU.shared)
        let layer = try DebugMaterialsCardViewModel.bundledLayer()
        let broken = WidgetMaterialLayer(
            material: layer.material,
            shader: .init(
                vertex: layer.vertexSource,
                fragment: layer.fragmentSource.replacingOccurrences(of: "metal::float4", with: "float5"),
                blockSizes: layer.blockSizes,
                readsTime: false,
                readsMotion: false
            ),
            uniforms: layer.uniforms,
            textures: layer.textures,
            arguments: layer.arguments
        )

        // When
        await #expect(throws: (any Error).self) {
            try await MaterialShaderCompiler.compile(broken, device: gpu.device, pixelFormat: .bgra8Unorm)
        }
    }

    /// The core's fragment stage reads `uv.y` from the facing of the quad, so a wrong winding turns
    /// the card upside down with no error.
    @Test func theTopRowOfTheBaseDrawsAtTheTop() async throws {
        // Given
        let gpu = try #require(MaterialGPU.shared)
        let layer = try DebugMaterialsCardViewModel.bundledLayer()
        let pipeline = try await gpu.pipeline(for: layer, pixelFormat: .bgra8Unorm)
        let base = try texture(gpu, format: .rgba8Unorm, rows: [[255, 0, 0, 255], [0, 0, 255, 255]])
        let noise = try texture(gpu, format: .r8Unorm, rows: [[0], [0]])
        let target = try texture(gpu, format: .bgra8Unorm, rows: [[0, 0, 0, 0], [0, 0, 0, 0]])
        let renderer = MaterialLayerRenderer(
            gpu: gpu,
            layers: [.init(layer: layer, pipeline: pipeline)],
            base: base,
            noise: noise
        )
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = target
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].storeAction = .store

        // When
        let commands = try #require(renderer.draw(into: pass, inputs: MaterialFrameInputs(resolution: .zero)))
        commands.commit()
        commands.waitUntilCompleted()

        // Then
        let top = pixel(target, row: 0)
        let bottom = pixel(target, row: 1)
        #expect(top.red > top.blue)
        #expect(bottom.blue > bottom.red)
    }

    /// The core's shaders read the base without premultiplication, and a swapped channel order
    /// would turn every card's colours.
    @Test func aSnapshotReachesTheGPUInRGBAOrderAndUnpremultiplied() throws {
        // Given
        let gpu = try #require(MaterialGPU.shared)
        let renderer = ImageRenderer(
            content: Color(.sRGB, red: 1, green: 0, blue: 0, opacity: 0.5).frame(width: 2, height: 2)
        )
        renderer.scale = 1
        renderer.isOpaque = false
        let snapshot = try #require(renderer.cgImage)

        // When
        let texture = try #require(gpu.texture(of: snapshot))

        // Then
        var rgba = [UInt8](repeating: 0, count: 4)
        texture.getBytes(&rgba, bytesPerRow: 4, from: MTLRegionMake2D(0, 0, 1, 1), mipmapLevel: 0)
        let expected: [UInt8] = [255, 0, 0, 128]
        #expect(zip(rgba, expected).allSatisfy { abs(Int($0) - Int($1)) <= 1 }, "read \(rgba)")
    }

    /// The last layer draws into the screen's format and the others into floats, so one material
    /// in both places needs two pipelines, and a cell that comes back on screen needs none.
    @Test func aLayerIsCompiledOncePerPixelFormat() async throws {
        // Given
        let gpu = try #require(MaterialGPU.shared)
        let layer = try DebugMaterialsCardViewModel.bundledLayer()

        // When
        let first = try await gpu.pipeline(for: layer, pixelFormat: .bgra8Unorm)
        let again = try await gpu.pipeline(for: layer, pixelFormat: .bgra8Unorm)
        let float = try await gpu.pipeline(for: layer, pixelFormat: .rgba32Float)

        // Then
        #expect(first === again)
        #expect(float !== first)
    }
}

private extension MaterialShaderCompilerTests {
    /// Two pixels wide, one row per entry of `rows`, each pixel the same.
    func texture(_ gpu: MaterialGPU, format: MTLPixelFormat, rows: [[UInt8]]) throws -> MTLTexture {
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: format,
            width: 2,
            height: rows.count,
            mipmapped: false
        )
        descriptor.usage = [.shaderRead, .renderTarget]
        descriptor.storageMode = .shared
        let texture = try #require(gpu.device.makeTexture(descriptor: descriptor))
        for (row, pixel) in rows.enumerated() {
            let bytes = pixel + pixel
            texture.replace(
                region: MTLRegionMake2D(0, row, 2, 1),
                mipmapLevel: 0,
                withBytes: bytes,
                bytesPerRow: bytes.count
            )
        }
        return texture
    }

    func pixel(_ texture: MTLTexture, row: Int) -> (red: UInt8, blue: UInt8) {
        var bgra = [UInt8](repeating: 0, count: 4)
        texture.getBytes(&bgra, bytesPerRow: 4, from: MTLRegionMake2D(0, row, 1, 1), mipmapLevel: 0)
        return (red: bgra[2], blue: bgra[0])
    }
}
