import Foundation
import Observation
import QuartzCore
import SwiftUI

/// What one `Materials` node of a card draws, as the host core resolved it.
/// A ready node draws its layers in order in place of its children.
public enum WidgetMaterialsState: Equatable, Sendable {
    case loading
    case ready([WidgetMaterialLayer])
    case plain
}

/// One layer ready to draw: the core's shaders and what feeds them.
public struct WidgetMaterialLayer: Equatable, Sendable, Decodable {
    public let material: Data
    public let vertexSource: String
    public let fragmentSource: String
    public let blockSizes: BlockSizes
    public let uniforms: [Uniform]
    public let textures: [TextureRole]
    public let readsTime: Bool
    public let readsMotion: Bool
    /// The tree's arguments in declaration order.
    public let arguments: [Argument]

    public init(
        material: Data,
        shader: Shader,
        uniforms: [Uniform],
        textures: [TextureRole],
        arguments: [Argument]
    ) {
        self.material = material
        vertexSource = shader.vertex
        fragmentSource = shader.fragment
        blockSizes = shader.blockSizes
        readsTime = shader.readsTime
        readsMotion = shader.readsMotion
        self.uniforms = uniforms
        self.textures = textures
        self.arguments = arguments
    }

    /// A layer that reads the clock or the finger's time draws every frame while on screen.
    public var isAnimated: Bool {
        readsTime || readsMotion || [.touchState, .elapsed, .replay].contains(where: arguments.contains)
    }

    public var readsTrail: Bool { arguments.contains(.trail) }

    /// Whether this host fills every texture slot. It loads no image argument other than the noise image
    /// and the trail.
    public var usesOnlyHostTextures: Bool {
        textures.allSatisfy { role in
            guard case let .argument(index) = role else { return true }
            return arguments.indices.contains(index) && [.noise, .trail].contains(arguments[index])
        }
    }
}

public extension WidgetMaterialLayer {
    struct Shader: Equatable, Sendable {
        public let vertex: String
        public let fragment: String
        public let blockSizes: BlockSizes
        public let readsTime: Bool
        public let readsMotion: Bool

        public init(vertex: String, fragment: String, blockSizes: BlockSizes, readsTime: Bool, readsMotion: Bool) {
            self.vertex = vertex
            self.fragment = fragment
            self.blockSizes = blockSizes
            self.readsTime = readsTime
            self.readsMotion = readsMotion
        }
    }

    struct BlockSizes: Equatable, Sendable, Decodable {
        public let vertex: Int
        public let fragment: Int

        public init(vertex: Int, fragment: Int) {
            self.vertex = vertex
            self.fragment = fragment
        }
    }

    struct Uniform: Equatable, Sendable, Decodable {
        public enum Stage: Equatable, Sendable, Decodable {
            case vertex
            case fragment
        }

        public enum Kind: Equatable, Sendable, Decodable {
            case float
            case int
            case vec2
            case vec3
            case vec4
            case mat3
        }

        public enum Role: Equatable, Sendable, Decodable {
            case time
            case tilt
            case resolution
            case orientation
            case rotation
            case argument(Int)
            case samplingMode(Int)
            case targetFlip
        }

        public let stage: Stage
        public let offset: Int
        public let kind: Kind
        public let role: Role

        public init(stage: Stage, offset: Int, kind: Kind, role: Role) {
            self.stage = stage
            self.offset = offset
            self.kind = kind
            self.role = role
        }
    }

    /// An argument as the tree gave it. Numbers are IEEE 754 bit patterns. The touch arguments, the
    /// replay clock and the trail come from the enclosing `Touch` node, and elapsed from the card's clock.
    enum Argument: Equatable, Sendable, Decodable {
        case numbers([UInt32])
        case noise
        case image
        case touch
        case touchState
        case elapsed
        case replay
        case trail
    }

    enum TextureRole: Equatable, Sendable, Decodable {
        case base
        case input
        case noise
        case argument(Int)
    }
}

/// The noise image of a body, `docs/rfcs/materials.md`, and the seed it was made from.
public struct WidgetMaterialNoise: Sendable {
    public let seed: UInt32
    public let bytes: Data

    public init(seed: UInt32, bytes: Data) {
        self.seed = seed
        self.bytes = bytes
    }
}

/// Why a ready node cannot draw on this device, and which of its layers it was.
public struct WidgetMaterialFailure: Sendable {
    public let node: UInt32
    public let material: Data
    public let reason: String
}

/// The resolved materials of one card, and where a node that cannot draw on this device is reported.
@MainActor
@Observable
public final class WidgetMaterials {
    public private(set) var nodes: [WidgetMaterialsState] = []
    public private(set) var noise: WidgetMaterialNoise?
    /// Counts the trees this card was given, so a node draws its new children even when its layers stay the same.
    public private(set) var generation = 0
    /// When the card was given its current tree, the zero of `elapsed` for the surfaces made for it.
    @ObservationIgnored public private(set) var treeStartedAt = CACurrentMediaTime()
    /// What each node draws, kept across trees so a node draws its last output while it loads.
    private var surfaces: [UInt32: MaterialSurface] = [:]
    @ObservationIgnored private let report: @MainActor (WidgetMaterialFailure) -> Void
    @ObservationIgnored private var reported: Set<UInt32> = []
    @ObservationIgnored private var touchFactory: WidgetTouchFactory?
    @ObservationIgnored private var touchAreas: [UInt32: WidgetTouchArea] = [:]

    public nonisolated init(report: @escaping @MainActor (WidgetMaterialFailure) -> Void) {
        self.report = report
    }

    public func state(of index: UInt32) -> WidgetMaterialsState {
        nodes.indices.contains(Int(index)) ? nodes[Int(index)] : .plain
    }

    public func update(nodes: [WidgetMaterialsState], noise: WidgetMaterialNoise?) {
        self.nodes = nodes
        self.noise = noise
    }

    /// A new tree of the same card, with the material of each layer of each node. The core keeps a
    /// reported node plain only for the resolution it was reported in, so a node that fails again is
    /// reported again. Every node loads until the first update, drawing the surface a node of the
    /// tree before left with the same materials, so it is never drawn plain in between and its old
    /// layers never draw with the new tree's clock.
    ///
    /// A `Touch` node keeps its trail, fired triggers and replay clock across trees while its props
    /// stay equal.
    public func startResolution(nodeMaterials: [[Data]], touch: WidgetTouchFactory? = nil) {
        reported = []
        generation += 1
        treeStartedAt = CACurrentMediaTime()
        surfaces = Self.carried(surfaces, materials: \.materials, into: nodeMaterials)
        nodes = Array(repeating: .loading, count: nodeMaterials.count)
        let keptKeys = touchFactory?.keys ?? []
        let newKeys = touch?.keys ?? []
        touchAreas = touchAreas.filter { index, _ in
            let position = Int(index)
            return keptKeys.indices.contains(position) && newKeys.indices.contains(position)
                && keptKeys[position] == newKeys[position]
        }
        touchFactory = touch
    }

    /// The outputs the nodes of a new tree start from. A node takes one a node with the same materials
    /// left, the one at its own index first, and each output goes to one node at most.
    static func carried<Output>(
        _ kept: [UInt32: Output],
        materials: (Output) -> [Data],
        into nodes: [[Data]]
    ) -> [UInt32: Output] {
        var left = kept
        var carried: [UInt32: Output] = [:]
        let indices = nodes.indices.map(UInt32.init)
        for index in indices {
            if let own = left[index], materials(own) == nodes[Int(index)] {
                carried[index] = left.removeValue(forKey: index)
            }
        }
        for index in indices where carried[index] == nil {
            let match = left.sorted { $0.key < $1.key }.first { materials($0.value) == nodes[Int(index)] }
            if let match { carried[index] = left.removeValue(forKey: match.key) }
        }
        return carried
    }

    func surface(of index: UInt32) -> MaterialSurface? {
        surfaces[index]
    }

    func keep(_ surface: MaterialSurface, for index: UInt32) {
        surfaces[index] = surface
    }

    func dropSurface(of index: UInt32) {
        surfaces[index] = nil
    }

    /// The state of `Touch` node `index` at `size`, made again when the size changes.
    public func touchArea(index: UInt32, size: CGSize) -> WidgetTouchArea? {
        guard let touchFactory, touchFactory.keys.indices.contains(Int(index)) else { return nil }
        if let kept = touchAreas[index], kept.size == size { return kept }
        let area = WidgetTouchArea(tracking: touchFactory.make(index, size), size: size)
        touchAreas[index] = area
        return area
    }

    /// Draws node `index` plain and tells the product, once per resolution.
    public func fail(node index: UInt32, material: Data, reason: String) {
        if nodes.indices.contains(Int(index)) {
            nodes[Int(index)] = .plain
        }
        dropSurface(of: index)
        guard reported.insert(index).inserted else { return }
        report(WidgetMaterialFailure(node: index, material: material, reason: reason))
    }
}

private struct WidgetMaterialsKey: EnvironmentKey {
    static let defaultValue: WidgetMaterials? = nil
}

extension EnvironmentValues {
    var widgetMaterials: WidgetMaterials? {
        get { self[WidgetMaterialsKey.self] }
        set { self[WidgetMaterialsKey.self] = newValue }
    }
}
