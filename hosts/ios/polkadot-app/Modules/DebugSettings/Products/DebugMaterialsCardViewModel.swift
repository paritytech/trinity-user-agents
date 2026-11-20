import Foundation
import Observation
import PolkadotUI
import Products
import SubstrateSdk
import TrUAPIHost

/// Draws one layer the core built from a material file, `DebugMaterialLayer.json`, over a card,
/// so the Metal path can be seen without a product or a material published on Bulletin.
///
/// The layer tints the card blue at the top and red at the bottom, so a picture drawn upside
/// down shows at a glance. The card ends in an image node, so a loaded picture can be seen
/// reaching the base.
@Observable
@MainActor
final class DebugMaterialsCardViewModel: WidgetNodeProviding {
    static let messageId = "debug-materials"
    static let noiseSeed: UInt32 = 0

    private(set) var node: CustomMessageWidgetNode?
    private(set) var refusal: String?
    @ObservationIgnored let materials: WidgetMaterials

    private let loadLayer: @Sendable () throws -> WidgetMaterialLayer
    private let resolver: any WidgetDesignTokenResolving

    init(
        loadLayer: @escaping @Sendable () throws -> WidgetMaterialLayer,
        resolver: any WidgetDesignTokenResolving,
        logger: LoggerProtocol
    ) {
        self.loadLayer = loadLayer
        self.resolver = resolver
        materials = WidgetMaterials { failure in
            logger.warning("Debug materials card draws plain: \(failure.reason)")
        }
    }

    func load() async {
        do {
            let layer = try loadLayer()
            let tree = try parseRendererNodeJson(json: Self.cardJson(material: layer.material))
            let noise = await Task.detached { materialNoise(seed: Self.noiseSeed) }.value
            node = tree.toWidgetNode(resolver: resolver)
            materials.startResolution(nodeCount: tree.materialsCount)
            materials.update(
                nodes: [.ready([layer])],
                noise: WidgetMaterialNoise(seed: Self.noiseSeed, bytes: noise)
            )
        } catch {
            refusal = "\(error)"
        }
    }
}

extension DebugMaterialsCardViewModel {
    static func bundledLayer() throws -> WidgetMaterialLayer {
        guard let url = Bundle.main.url(forResource: "DebugMaterialLayer", withExtension: "json") else {
            throw CocoaError(.fileNoSuchFile)
        }
        return try JSONDecoder().decode(WidgetMaterialLayer.self, from: Data(contentsOf: url))
    }
}

private extension DebugMaterialsCardViewModel {
    static func cardJson(material: Data) -> String {
        """
        {"tag":"Materials","value":{"props":{"layers":[{"material":"\(material.toHex(includePrefix: true))",
        "args":[]}]},
        "children":[{"tag":"Column","value":{"modifiers":[{"tag":"FillWidth","value":true},
        {"tag":"Padding","value":{"top":24,"bottom":24,"start":24,"end":24}}],
        "props":{"horizontalAlignment":null,"verticalArrangement":null},"children":[
        {"tag":"Text","value":{"modifiers":[],"props":{"style":null,"color":null},
        "children":[{"tag":"String","value":{"text":"Top: blue"}}]}},
        {"tag":"Text","value":{"modifiers":[],"props":{"style":null,"color":null},
        "children":[{"tag":"String","value":{"text":"Bottom: red"}}]}},
        {"tag":"Image","value":{"modifiers":[{"tag":"Width","value":48},{"tag":"Height","value":48}],
        "props":{"source":{"tag":"Archive","value":"debug-picture.png"},"fit":null}}}]}}]}}
        """
    }
}
