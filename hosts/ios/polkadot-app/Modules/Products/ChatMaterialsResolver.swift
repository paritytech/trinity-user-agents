import Foundation
import PolkadotUI
import SubstrateSdk
import TrUAPIHost

/// One resolution of a tree's materials, as the core streams it.
protocol MaterialsResolving: AnyObject, Sendable {
    var resolved: AsyncStream<ResolvedMaterials> { get }
    func report(node: UInt32, material: Bytes32?, reason: MaterialFailure)
    func cancel()
}

extension MaterialsResolution: MaterialsResolving {}

/// Follows the materials of the tree a chat card draws, and publishes them to the card.
@MainActor
final class ChatMaterialsResolver {
    nonisolated let materials: WidgetMaterials

    private let resolve: (RendererNode) -> (any MaterialsResolving)?
    private let noiseCache: MaterialNoiseCache
    private let reporter: MaterialFailureReporter
    private var current: (any MaterialsResolving)?
    private var following: Task<Void, Never>?

    nonisolated init(
        resolve: @escaping (RendererNode) -> (any MaterialsResolving)?,
        logger: LoggerProtocol,
        noiseCache: MaterialNoiseCache = .shared
    ) {
        let reporter = MaterialFailureReporter(logger: logger)
        materials = WidgetMaterials { failure in reporter.report(failure) }
        self.resolve = resolve
        self.noiseCache = noiseCache
        self.reporter = reporter
    }

    deinit {
        following?.cancel()
        current?.cancel()
    }

    /// The new tree is resolved before the old resolution is cancelled, so a material both share
    /// stays loaded.
    func follow(tree: RendererNode) {
        let previous = current
        current = resolve(tree)
        reporter.resolution = current
        following?.cancel()
        previous?.cancel()
        guard let current else {
            materials.update(nodes: [], noise: nil)
            return
        }
        materials.startResolution(nodeCount: tree.materialsCount)
        following = Task { [weak self, noiseCache] in
            for await update in current.resolved {
                let noise = await noiseCache.noise(for: update)
                guard !Task.isCancelled else { return }
                self?.materials.update(nodes: update.nodes.map(\.widgetState), noise: noise)
            }
        }
    }
}

/// Logs a node the card cannot draw, and tells the resolution it came from.
@MainActor
private final class MaterialFailureReporter {
    weak var resolution: (any MaterialsResolving)?
    private let logger: LoggerProtocol

    nonisolated init(logger: LoggerProtocol) {
        self.logger = logger
    }

    func report(_ failure: WidgetMaterialFailure) {
        logger.warning(
            "Materials node \(failure.node) draws plain, material \(failure.material.toHex(includePrefix: true)): "
                + failure.reason
        )
        resolution?.report(node: failure.node, material: failure.material, reason: .unsupported)
    }
}

/// The noise images of the bodies on screen, made off the main thread once per seed and shared
/// by every card. Each is 1 MiB, so only the last few seeds are kept.
@MainActor
final class MaterialNoiseCache {
    static let shared = MaterialNoiseCache()

    private static let kept = 4
    private let makeNoise: @Sendable (UInt32) -> Data
    private var images: [(seed: UInt32, image: Task<WidgetMaterialNoise, Never>)] = []

    nonisolated init(makeNoise: @escaping @Sendable (UInt32) -> Data = materialNoise(seed:)) {
        self.makeNoise = makeNoise
    }

    /// Nil while no node is ready, so a card that never draws makes no image.
    func noise(for update: ResolvedMaterials) async -> WidgetMaterialNoise? {
        guard update.nodes.contains(where: { if case .ready = $0 { true } else { false } }) else { return nil }
        let seed = update.noiseSeed
        if let kept = images.first(where: { $0.seed == seed }) { return await kept.image.value }
        let makeNoise = makeNoise
        let image = Task.detached { WidgetMaterialNoise(seed: seed, bytes: makeNoise(seed)) }
        images.append((seed, image))
        if images.count > Self.kept { images.removeFirst() }
        return await image.value
    }
}

extension NodeMaterials {
    var widgetState: WidgetMaterialsState {
        switch self {
        case .loading: .loading
        case let .ready(layers): .ready(layers.map(\.widgetLayer))
        case .plain: .plain
        }
    }
}

extension ReadyLayer {
    var widgetLayer: WidgetMaterialLayer {
        WidgetMaterialLayer(
            material: material,
            shader: .init(
                vertex: shader.vertex,
                fragment: shader.fragment,
                blockSizes: .init(vertex: Int(shader.vertexBlockSize), fragment: Int(shader.fragmentBlockSize)),
                readsTime: shader.readsTime,
                readsMotion: shader.readsMotion
            ),
            uniforms: shader.uniforms.map(\.widgetUniform),
            textures: shader.textures.map(\.role.widgetRole),
            arguments: args.map(\.widgetArgument)
        )
    }
}

private extension Uniform {
    var widgetUniform: WidgetMaterialLayer.Uniform {
        WidgetMaterialLayer.Uniform(
            stage: stage == .vertex ? .vertex : .fragment,
            offset: Int(offset),
            kind: kind.widgetKind,
            role: role.widgetRole
        )
    }
}

private extension UniformKind {
    var widgetKind: WidgetMaterialLayer.Uniform.Kind {
        switch self {
        case .float: .float
        case .int: .int
        case .vec2: .vec2
        case .vec3: .vec3
        case .vec4: .vec4
        case .mat3: .mat3
        }
    }
}

private extension UniformRole {
    var widgetRole: WidgetMaterialLayer.Uniform.Role {
        switch self {
        case .time: .time
        case .tilt: .tilt
        case .resolution: .resolution
        case .orientation: .orientation
        case .rotation: .rotation
        case let .argument(index): .argument(Int(index))
        case let .samplingMode(slot): .samplingMode(Int(slot))
        case .targetFlip: .targetFlip
        }
    }
}

private extension TextureRole {
    var widgetRole: WidgetMaterialLayer.TextureRole {
        switch self {
        case .base: .base
        case .input: .input
        case .noise: .noise
        case let .argument(index): .argument(Int(index))
        }
    }
}

private extension Parameter {
    var widgetArgument: WidgetMaterialLayer.Argument {
        switch self {
        case let .float(bits): .numbers([bits])
        case let .vec2(bits),
             let .vec3(bits),
             let .vec4(bits):
            .numbers(bits)
        case let .samplerFloat(image),
             let .samplerVec2(image),
             let .samplerVec3(image),
             let .samplerVec4(image):
            if case .noise = image { .noise } else { .image }
        }
    }
}
