import Metal

/// Draws a node's layers in order: each reads the children as `Base` and the previous output as `Input`,
/// and the last draws into the view.
@MainActor
final class MaterialLayerRenderer {
    struct CompiledLayer {
        let layer: WidgetMaterialLayer
        let pipeline: MTLRenderPipelineState
    }

    /// The quad in clip space, counter-clockwise. `TargetFlip` mirrors it into a top-down target, which
    /// turns it clockwise, so with counter-clockwise as the front it faces back as the core expects.
    private static let quad: [SIMD2<Float>] = [[-1, -1], [1, -1], [-1, 1], [1, 1]]

    let gpu: MaterialGPU
    private let layers: [CompiledLayer]
    private let base: MTLTexture
    private let noise: MTLTexture
    private var outputs: [MTLTexture] = []
    private var trail: (revision: UInt64, texture: MTLTexture)?

    init(gpu: MaterialGPU, layers: [CompiledLayer], base: MTLTexture, noise: MTLTexture) {
        self.gpu = gpu
        self.layers = layers
        self.base = base
        self.noise = noise
    }

    /// `trailImage` is the enclosing `Touch` node's trail, uploaded again only when its revision moved.
    func draw(
        into target: MTLRenderPassDescriptor,
        inputs: MaterialFrameInputs,
        trailImage: WidgetTrailImage? = nil
    ) -> MTLCommandBuffer? {
        if let trailImage, trailImage.revision != trail?.revision || trail == nil,
           let texture = gpu.trailTexture(trailImage, reusing: trail?.texture) {
            trail = (trailImage.revision, texture)
        }
        guard let commands = gpu.commandQueue.makeCommandBuffer() else { return nil }
        var input = base
        for (position, compiled) in layers.enumerated() {
            let isLast = position == layers.count - 1
            let pass = isLast ? target : intermediatePass(position)
            guard let pass, let encoder = commands.makeRenderCommandEncoder(descriptor: pass) else { return nil }
            encode(compiled, input: input, inputs: inputs, encoder: encoder)
            encoder.endEncoding()
            if !isLast, let output = pass.colorAttachments[0].texture {
                input = output
            }
        }
        return commands
    }
}

private extension MaterialLayerRenderer {
    func intermediatePass(_ position: Int) -> MTLRenderPassDescriptor? {
        let slot = position % 2
        if outputs.count <= slot {
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(
                pixelFormat: .rgba32Float,
                width: base.width,
                height: base.height,
                mipmapped: false
            )
            descriptor.usage = [.renderTarget, .shaderRead]
            guard let texture = gpu.device.makeTexture(descriptor: descriptor) else { return nil }
            outputs.append(texture)
        }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = outputs[slot]
        pass.colorAttachments[0].loadAction = .dontCare
        pass.colorAttachments[0].storeAction = .store
        return pass
    }

    func encode(
        _ compiled: CompiledLayer,
        input: MTLTexture,
        inputs: MaterialFrameInputs,
        encoder: MTLRenderCommandEncoder
    ) {
        var frame = inputs
        frame.resolution = [Float(input.width), Float(input.height)]
        let textures = compiled.layer.textures.map { texture(for: $0, layer: compiled.layer, input: input) }
        for (slot, texture) in textures.enumerated() where !gpu.floatFilterable && texture.pixelFormat == .rgba32Float {
            frame.samplingModes[slot] = 1
        }
        let blocks = MaterialUniformPacker.pack(compiled.layer, inputs: frame)

        encoder.setRenderPipelineState(compiled.pipeline)
        encoder.setCullMode(.none)
        encoder.setFrontFacing(.counterClockwise)
        encoder.setVertexBytes(Self.quad, length: MemoryLayout<SIMD2<Float>>.stride * 4, index: 2)
        set(blocks.vertex, index: MaterialShaderCompiler.vertexUniformsBuffer, vertex: true, encoder: encoder)
        set(blocks.fragment, index: MaterialShaderCompiler.fragmentUniformsBuffer, vertex: false, encoder: encoder)
        for (slot, role) in compiled.layer.textures.enumerated() {
            encoder.setFragmentTexture(textures[slot], index: slot)
            encoder.setFragmentSamplerState(
                sampler(for: role, layer: compiled.layer, texture: textures[slot]),
                index: slot
            )
        }
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
    }

    func set(_ block: Data, index: Int, vertex: Bool, encoder: MTLRenderCommandEncoder) {
        guard !block.isEmpty else { return }
        block.withUnsafeBytes { bytes in
            guard let address = bytes.baseAddress else { return }
            if vertex {
                encoder.setVertexBytes(address, length: bytes.count, index: index)
            } else {
                encoder.setFragmentBytes(address, length: bytes.count, index: index)
            }
        }
    }

    func texture(
        for role: WidgetMaterialLayer.TextureRole,
        layer: WidgetMaterialLayer,
        input: MTLTexture
    ) -> MTLTexture {
        switch role {
        case .base: base
        case .input: input
        case .noise: noise
        case let .argument(index): readsTrail(layer, index) ? trail?.texture ?? noise : noise
        }
    }

    /// Base, Input and the trail linear with clamped edges, the noise image nearest and repeated,
    /// `rust/crates/truapi/src/materials/shader.rs`. A float image the GPU cannot filter is read
    /// nearest, and the shader filters it.
    func sampler(
        for role: WidgetMaterialLayer.TextureRole,
        layer: WidgetMaterialLayer,
        texture: MTLTexture
    ) -> MTLSamplerState {
        switch role {
        case .base,
             .input:
            !gpu.floatFilterable && texture.pixelFormat == .rgba32Float ? gpu.nearestClamped : gpu.linearClamped
        case .noise:
            gpu.nearestRepeated
        case let .argument(index):
            readsTrail(layer, index) ? gpu.linearClamped : gpu.nearestRepeated
        }
    }

    func readsTrail(_ layer: WidgetMaterialLayer, _ index: Int) -> Bool {
        layer.arguments.indices.contains(index) && layer.arguments[index] == .trail
    }
}
