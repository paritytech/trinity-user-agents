import Metal
import MetalKit

/// Draws coins with the reference's own material: measured-optics metals lit against a prefiltered
/// studio, relief from a baked normal atlas, wear that hazes the fields and leaves grime in the
/// struck figure.
///
/// One instanced draw per (geometry, level of detail) in use, which is five to ten in practice and
/// twenty-eight at worst. Everything else is a single pass.
final class CoinageMetalRenderer {
    /// What one coin hands the GPU: four `float4`, interleaved, 64 bytes.
    struct Instance {
        /// World x, world y (up, so the caller negates screen y), lift z, height. All in points.
        var position: SIMD4<Float>
        /// Turn about y, tilt about x, spin about z, thickness as a multiple of the base.
        var rotation: SIMD4<Float>
        /// Wear, outer metal index, core metal index or -1, relief tile.
        var look: SIMD4<Float>
        /// Reeds, luster, recede, edge calm.
        var effects: SIMD4<Float>
        /// Pits (one per hop or split), how streaked the face is, and the coin's own seed.
        var marks: SIMD4<Float>
    }

    /// A run of instances sharing a mesh, which is what an instanced draw needs.
    struct Batch {
        let geometry: String
        var instances: [Instance]
    }

    enum Failure: Error {
        case noDevice
        case shaderMissing(String)
    }

    let device: MTLDevice
    let store: CoinageAssetStore

    private let queue: MTLCommandQueue
    private let pipeline: MTLRenderPipelineState
    private let depthState: MTLDepthStencilState
    private let sampler: MTLSamplerState
    private var instanceRing: [MTLBuffer]
    private var ringSlot = 0
    /// Holds the CPU back when it is a whole ring ahead of the GPU. Without it the instances being
    /// written are the ones a command buffer in flight is still reading.
    private let framesAvailable = DispatchSemaphore(value: framesInFlight)
    private var hasGeneratedMipmaps = false

    /// Three frames in flight, so the CPU can write next frame's instances while the GPU reads this
    /// one's. `setVertexBytes` would cap a draw at sixty-four coins.
    private static let framesInFlight = 3

    /// What the instance ring starts at. It grows to fit rather than dropping coins on the floor.
    private static let initialCoins = 600

    /// Multisampling, if the device has it. Asked once and used for both the pipeline and the view:
    /// a pipeline built for four samples against a render pass with one is a validation failure at
    /// the draw call, and nothing before it would say so.
    private(set) var sampleCount = 1

    init(bundle: Bundle = .main) throws {
        guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {
            throw Failure.noDevice
        }

        self.device = device
        self.queue = queue
        store = try CoinageAssetStore(device: device, bundle: bundle)

        guard let library = try? device.makeDefaultLibrary(bundle: bundle),
              let vertex = library.makeFunction(name: "coinVertex"),
              let fragment = library.makeFunction(name: "coinFragment")
        else {
            throw Failure.shaderMissing("coinVertex/coinFragment")
        }

        sampleCount = Self.sampleCount(for: device)
        pipeline = try Self.makePipeline(
            device: device,
            vertex: vertex,
            fragment: fragment,
            sampleCount: sampleCount
        )
        depthState = Self.makeDepthState(device: device)
        sampler = Self.makeSampler(device: device)

        instanceRing = (0 ..< Self.framesInFlight).compactMap {
            Self.makeInstanceBuffer(device: device, coins: Self.initialCoins, slot: $0)
        }
    }

    /// Enough room for every coin, grown if a wallet outgrows it.
    ///
    /// A fixed ceiling meant a large enough wallet silently lost whatever did not fit, which is a
    /// worse answer than a slightly larger allocation.
    private func ensureCapacity(_ coins: Int) {
        let needed = coins * MemoryLayout<Instance>.stride

        guard let current = instanceRing.first, current.length < needed else { return }

        instanceRing = (0 ..< Self.framesInFlight).compactMap {
            Self.makeInstanceBuffer(device: device, coins: coins * 2, slot: $0)
        }
    }

    private static func makeInstanceBuffer(device: MTLDevice, coins: Int, slot: Int) -> MTLBuffer? {
        let buffer = device.makeBuffer(
            length: MemoryLayout<Instance>.stride * max(coins, 1),
            options: .storageModeShared
        )
        buffer?.label = "coin instances \(slot)"

        return buffer
    }

    func draw(
        _ batches: [Batch],
        in view: MTKView,
        viewport: CGSize,
        dpr: CGFloat,
        light: CoinageTilt.Turn
    ) {
        ensureCapacity(batches.reduce(0) { $0 + $1.instances.count })

        guard let descriptor = view.currentRenderPassDescriptor,
              let drawable = view.currentDrawable,
              let command = queue.makeCommandBuffer()
        else {
            return
        }

        framesAvailable.wait()
        command.addCompletedHandler { [framesAvailable] _ in framesAvailable.signal() }

        // The atlas ships without mips, and the box filter has to be the GPU's, unnormalised.
        if !hasGeneratedMipmaps, let blit = command.makeBlitCommandEncoder() {
            blit.generateMipmaps(for: store.reliefAtlas)
            blit.endEncoding()
            hasGeneratedMipmaps = true
        }

        var params = store.params
        params.viewportWidth = Float(viewport.width)
        params.viewportHeight = Float(viewport.height)
        params.dpr = Float(dpr)
        params.lightTurn = SIMD3(Float(light.x), Float(light.y), Float(light.z))

        let packed = params.packed()
        let metals = store.metalRows

        if let encoder = command.makeRenderCommandEncoder(descriptor: descriptor) {
            encode(batches, into: encoder, params: packed, metals: metals)
            encoder.endEncoding()
        }

        command.present(drawable)
        command.commit()

        ringSlot = (ringSlot + 1) % Self.framesInFlight
    }
}

// MARK: - Encoding

extension CoinageMetalRenderer {
    /// The samples to build the pipeline for.
    ///
    /// Also asked before there is a renderer, because the view has to be configured the moment it
    /// is made and a view and a pipeline that disagree on samples fail validation at the draw call
    /// with nothing to say why. Asked of the device both times rather than remembered, so the two
    /// answers cannot drift apart.
    static func sampleCount(for device: MTLDevice?) -> Int {
        device?.supportsTextureSampleCount(4) == true ? 4 : 1
    }
}

private extension CoinageMetalRenderer {
    func encode(
        _ batches: [Batch],
        into encoder: MTLRenderCommandEncoder,
        params: [Float],
        metals: [Float]
    ) {
        guard let ring = instanceRing[safe: ringSlot] else { return }

        encoder.setRenderPipelineState(pipeline)
        encoder.setDepthStencilState(depthState)
        encoder.setFrontFacing(.counterClockwise)
        encoder.setCullMode(.back)
        encoder.setVertexBytes(params, length: params.count * MemoryLayout<Float>.size, index: 4)
        encoder.setFragmentBytes(params, length: params.count * MemoryLayout<Float>.size, index: 4)
        encoder.setFragmentBytes(metals, length: metals.count * MemoryLayout<Float>.size, index: 5)
        encoder.setFragmentTexture(store.reliefAtlas, index: 0)
        encoder.setFragmentTexture(store.environment, index: 1)
        encoder.setFragmentSamplerState(sampler, index: 0)

        let stride = MemoryLayout<Instance>.stride
        var offset = 0

        for batch in batches {
            // The ring was sized for every coin in this frame, so a batch that does not fit means
            // the sizing and the encoding disagree. Stopping is right: carrying on would write past
            // the buffer, and dropping this one alone would leave a hole with no explanation.
            guard offset + batch.instances.count * stride <= ring.length else {
                assertionFailure("coin instances outgrew their buffer mid-frame")
                break
            }

            guard !batch.instances.isEmpty,
                  let mesh = try? store.mesh(geometry: batch.geometry)
            else {
                continue
            }

            batch.instances.withUnsafeBytes {
                ring.contents().advanced(by: offset).copyMemory(
                    from: $0.baseAddress!,
                    byteCount: $0.count
                )
            }

            encoder.setVertexBuffer(mesh.positions, offset: 0, index: 0)
            encoder.setVertexBuffer(mesh.normals, offset: 0, index: 1)
            encoder.setVertexBuffer(mesh.surfaces, offset: 0, index: 2)
            encoder.setVertexBuffer(ring, offset: offset, index: 3)
            encoder.drawIndexedPrimitives(
                type: .triangle,
                indexCount: mesh.indexCount,
                indexType: .uint32,
                indexBuffer: mesh.indices,
                indexBufferOffset: 0,
                instanceCount: batch.instances.count
            )

            offset += batch.instances.count * stride
        }
    }

    static func makePipeline(
        device: MTLDevice,
        vertex: MTLFunction,
        fragment: MTLFunction,
        sampleCount: Int
    ) throws -> MTLRenderPipelineState {
        let layout = MTLVertexDescriptor()

        func attribute(_ index: Int, _ format: MTLVertexFormat, buffer: Int, offset: Int) {
            layout.attributes[index].format = format
            layout.attributes[index].bufferIndex = buffer
            layout.attributes[index].offset = offset
        }

        attribute(0, .float3, buffer: 0, offset: 0)
        attribute(1, .float3, buffer: 1, offset: 0)
        attribute(2, .float4, buffer: 2, offset: 0)

        for slot in 0 ..< 5 {
            attribute(3 + slot, .float4, buffer: 3, offset: slot * 16)
        }

        layout.layouts[0].stride = 12
        layout.layouts[1].stride = 12
        layout.layouts[2].stride = 16
        layout.layouts[3].stride = 80
        layout.layouts[3].stepFunction = .perInstance
        layout.layouts[3].stepRate = 1

        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = vertex
        descriptor.fragmentFunction = fragment
        descriptor.vertexDescriptor = layout
        descriptor.rasterSampleCount = sampleCount
        // Not an sRGB target: the shader encodes sRGB itself.
        descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm
        descriptor.depthAttachmentPixelFormat = .depth32Float

        return try device.makeRenderPipelineState(descriptor: descriptor)
    }

    static func makeDepthState(device: MTLDevice) -> MTLDepthStencilState {
        let descriptor = MTLDepthStencilDescriptor()
        descriptor.depthCompareFunction = .less
        descriptor.isDepthWriteEnabled = true

        // A depth state is refused only for a descriptor this one cannot be.
        return device.makeDepthStencilState(descriptor: descriptor)!
    }

    static func makeSampler(device: MTLDevice) -> MTLSamplerState {
        let descriptor = MTLSamplerDescriptor()
        descriptor.minFilter = .linear
        descriptor.magFilter = .linear
        descriptor.mipFilter = .linear
        descriptor.sAddressMode = .clampToEdge
        descriptor.tAddressMode = .clampToEdge
        descriptor.rAddressMode = .clampToEdge

        return device.makeSamplerState(descriptor: descriptor)!
    }
}
