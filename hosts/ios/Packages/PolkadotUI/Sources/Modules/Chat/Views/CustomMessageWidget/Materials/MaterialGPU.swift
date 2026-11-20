import Accelerate
import CoreGraphics
import Metal

/// The GPU state every material card shares: one device and command queue, the three samplers the
/// core's shaders expect, and the pipelines and noise images already made, so a cell that comes
/// back on screen compiles nothing.
@MainActor
public final class MaterialGPU {
    /// Nil on a device without Metal, where every node draws plain.
    public static let shared = MaterialGPU(device: MTLCreateSystemDefaultDevice())

    /// Whether this device filters 32-bit float textures, which decides the shaders the core builds.
    public nonisolated static let floatFilterable = MTLCreateSystemDefaultDevice()?.supports32BitFloatFiltering ?? false

    let device: MTLDevice
    let commandQueue: MTLCommandQueue
    let floatFilterable: Bool
    let linearClamped: MTLSamplerState
    let nearestClamped: MTLSamplerState
    let nearestRepeated: MTLSamplerState
    private var pipelines: [(key: PipelineKey, pipeline: Task<MTLRenderPipelineState, Error>)] = []
    private var noiseTexture: (seed: UInt32, texture: MTLTexture)?

    init?(device: MTLDevice?) {
        guard let device,
              let commandQueue = device.makeCommandQueue(),
              let linearClamped = device.makeSamplerState(descriptor: .material(.linear, .clampToEdge)),
              let nearestClamped = device.makeSamplerState(descriptor: .material(.nearest, .clampToEdge)),
              let nearestRepeated = device.makeSamplerState(descriptor: .material(.nearest, .repeat))
        else { return nil }
        self.device = device
        self.commandQueue = commandQueue
        floatFilterable = device.supports32BitFloatFiltering
        self.linearClamped = linearClamped
        self.nearestClamped = nearestClamped
        self.nearestRepeated = nearestRepeated
    }

    /// A failed compile is dropped, so the next resolution of the node tries again. Only the
    /// pipelines of the last few layers are kept.
    func pipeline(for layer: WidgetMaterialLayer, pixelFormat: MTLPixelFormat) async throws -> MTLRenderPipelineState {
        let key = PipelineKey(
            material: layer.material,
            fragmentLength: layer.fragmentSource.utf8.count,
            pixelFormat: pixelFormat.rawValue
        )
        if let kept = pipelines.first(where: { $0.key == key }) { return try await kept.pipeline.value }
        let device = device
        let compiling = Task {
            try await MaterialShaderCompiler.compile(layer, device: device, pixelFormat: pixelFormat)
        }
        pipelines.append((key, compiling))
        if pipelines.count > Self.keptPipelines { pipelines.removeFirst() }
        do {
            return try await compiling.value
        } catch {
            pipelines.removeAll { $0.key == key }
            throw error
        }
    }

    func noise(_ noise: WidgetMaterialNoise) -> MTLTexture? {
        if let noiseTexture, noiseTexture.seed == noise.seed { return noiseTexture.texture }
        guard let texture = Self.texture(noise.bytes, device: device) else { return nil }
        noiseTexture = (noise.seed, texture)
        return texture
    }

    /// The image redrawn as 8-bit sRGB, its colour divided by its alpha again after the draw, since
    /// the core's shaders read every image without premultiplication. `MTKTextureLoader` refuses the
    /// images `ImageRenderer` makes, "Image decoding failed".
    func texture(of image: CGImage) -> MTLTexture? {
        let bytesPerRow = image.width * 4
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                  data: nil,
                  width: image.width,
                  height: image.height,
                  bitsPerComponent: 8,
                  bytesPerRow: bytesPerRow,
                  space: space,
                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              )
        else { return nil }
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        guard let pixels = context.data else { return nil }
        var buffer = vImage_Buffer(
            data: pixels,
            height: vImagePixelCount(image.height),
            width: vImagePixelCount(image.width),
            rowBytes: bytesPerRow
        )
        guard vImageUnpremultiplyData_RGBA8888(&buffer, &buffer, vImage_Flags(kvImageNoFlags)) == kvImageNoError
        else { return nil }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .rgba8Unorm,
            width: image.width,
            height: image.height,
            mipmapped: false
        )
        descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
        texture.replace(
            region: MTLRegionMake2D(0, 0, image.width, image.height),
            mipmapLevel: 0,
            withBytes: pixels,
            bytesPerRow: bytesPerRow
        )
        return texture
    }
}

private struct PipelineKey: Hashable {
    let material: Data
    /// The core builds another source for the same material when the device cannot filter floats.
    let fragmentLength: Int
    let pixelFormat: UInt
}

private extension MaterialGPU {
    static let noiseSide = 1_024
    /// One tree names at most 16 materials, and each can draw in two formats.
    static let keptPipelines = 32

    static func texture(_ bytes: Data, device: MTLDevice) -> MTLTexture? {
        guard bytes.count == noiseSide * noiseSide else { return nil }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .r8Unorm,
            width: noiseSide,
            height: noiseSide,
            mipmapped: false
        )
        descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
        bytes.withUnsafeBytes { pixels in
            guard let address = pixels.baseAddress else { return }
            texture.replace(
                region: MTLRegionMake2D(0, 0, noiseSide, noiseSide),
                mipmapLevel: 0,
                withBytes: address,
                bytesPerRow: noiseSide
            )
        }
        return texture
    }
}

private extension MTLSamplerDescriptor {
    static func material(_ filter: MTLSamplerMinMagFilter, _ address: MTLSamplerAddressMode) -> MTLSamplerDescriptor {
        let descriptor = MTLSamplerDescriptor()
        descriptor.minFilter = filter
        descriptor.magFilter = filter
        descriptor.sAddressMode = address
        descriptor.tAddressMode = address
        return descriptor
    }
}
