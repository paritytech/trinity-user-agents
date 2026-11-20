import Metal

/// Compiles the Metal source the core writes for a layer into a pipeline.
enum MaterialShaderCompiler {
    /// The entry points and bindings the core's Metal output uses, `rust/crates/truapi/src/materials/metal.rs`.
    static let vertexFunction = "material_vertex"
    static let fragmentFunction = "material_fragment"
    static let fragmentUniformsBuffer = 0
    static let vertexUniformsBuffer = 1
    static let quadBuffer = 2

    enum Failure: Error {
        case missingFunction
    }

    /// Off the main thread: the source is compiled at run time.
    static func compile(
        _ layer: WidgetMaterialLayer,
        device: MTLDevice,
        pixelFormat: MTLPixelFormat
    ) async throws -> MTLRenderPipelineState {
        let vertexLibrary = try await device.makeLibrary(source: layer.vertexSource, options: nil)
        let fragmentLibrary = try await device.makeLibrary(source: layer.fragmentSource, options: nil)
        guard let vertex = vertexLibrary.makeFunction(name: vertexFunction),
              let fragment = fragmentLibrary.makeFunction(name: fragmentFunction)
        else { throw Failure.missingFunction }

        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = vertex
        descriptor.fragmentFunction = fragment
        descriptor.vertexDescriptor = quadLayout()
        descriptor.colorAttachments[0].pixelFormat = pixelFormat
        if pixelFormat != .rgba32Float {
            blendOverClear(descriptor.colorAttachments[0])
        }
        return try await device.makeRenderPipelineState(descriptor: descriptor)
    }
}

private extension MaterialShaderCompiler {
    static func quadLayout() -> MTLVertexDescriptor {
        let layout = MTLVertexDescriptor()
        layout.attributes[0].format = .float2
        layout.attributes[0].bufferIndex = quadBuffer
        layout.attributes[0].offset = 0
        layout.layouts[quadBuffer].stride = MemoryLayout<SIMD2<Float>>.stride
        return layout
    }

    /// The layer's output is not premultiplied. Over a clear drawable this leaves it premultiplied,
    /// which is what Core Animation composites.
    static func blendOverClear(_ attachment: MTLRenderPipelineColorAttachmentDescriptor) {
        attachment.isBlendingEnabled = true
        attachment.sourceRGBBlendFactor = .sourceAlpha
        attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha
        attachment.sourceAlphaBlendFactor = .one
        attachment.destinationAlphaBlendFactor = .oneMinusSourceAlpha
    }
}
