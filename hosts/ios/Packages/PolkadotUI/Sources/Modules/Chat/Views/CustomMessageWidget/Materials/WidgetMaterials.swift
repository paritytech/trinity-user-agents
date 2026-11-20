import Foundation
import Observation
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

    public var isAnimated: Bool { readsTime || readsMotion }

    /// Whether this host fills every texture slot. It loads no image argument other than the noise image.
    public var usesOnlyHostTextures: Bool {
        textures.allSatisfy { role in
            guard case let .argument(index) = role else { return true }
            return arguments.indices.contains(index) && arguments[index] == .noise
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

    /// An argument as the tree gave it. Numbers are IEEE 754 bit patterns.
    enum Argument: Equatable, Sendable, Decodable {
        case numbers([UInt32])
        case noise
        case image
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
    @ObservationIgnored private let report: @MainActor (WidgetMaterialFailure) -> Void
    @ObservationIgnored private var reported: Set<UInt32> = []

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

    /// A new tree of the same card. The core keeps a reported node plain only for the resolution
    /// it was reported in, so a node that fails again is reported again. A tree with another number
    /// of nodes draws them plain until its first update, since an index may now name another node.
    public func startResolution(nodeCount: Int) {
        reported = []
        generation += 1
        if nodes.count != nodeCount {
            nodes = Array(repeating: .plain, count: nodeCount)
        }
    }

    /// Draws node `index` plain and tells the product, once per resolution.
    public func fail(node index: UInt32, material: Data, reason: String) {
        if nodes.indices.contains(Int(index)) {
            nodes[Int(index)] = .plain
        }
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
