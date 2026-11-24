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
///
/// Under it, a scratch card, `DebugScratchLayer.json`, opens where the finger passes and turns
/// into its prize at 60 percent, and a slider, `DebugSliderLayer.json`, follows the finger and
/// turns into its confirmation at the end, each by a new tree as a product would send it.
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
    private let logger: LoggerProtocol
    private var layers: DebugLayers?
    private var noise: WidgetMaterialNoise?
    private var revealed = false
    private var confirmed = false

    init(
        loadLayer: @escaping @Sendable () throws -> WidgetMaterialLayer,
        resolver: any WidgetDesignTokenResolving,
        logger: LoggerProtocol
    ) {
        self.loadLayer = loadLayer
        self.resolver = resolver
        self.logger = logger
        materials = WidgetMaterials { failure in
            logger.warning("Debug materials card draws plain: \(failure.reason)")
        }
    }

    func load() async {
        do {
            layers = try DebugLayers(
                card: loadLayer(),
                scratch: Self.bundledLayer(named: "DebugScratchLayer"),
                slider: Self.bundledLayer(named: "DebugSliderLayer")
            )
            let seed = Self.noiseSeed
            let bytes = await Task.detached { materialNoise(seed: seed) }.value
            noise = WidgetMaterialNoise(seed: seed, bytes: bytes)
            try render()
        } catch {
            refusal = "\(error)"
        }
    }
}

private struct DebugLayers {
    let card: WidgetMaterialLayer
    let scratch: WidgetMaterialLayer
    let slider: WidgetMaterialLayer
}

private extension DebugMaterialsCardViewModel {
    func render() throws {
        guard let layers else { return }
        let tree = try parseRendererNodeJson(json: Self.cardsJson(
            layers: layers,
            revealed: revealed,
            confirmed: confirmed
        ))
        node = tree.toWidgetNode(resolver: resolver)
        let props = tree.touchProps
        let factory = WidgetTouchFactory(keys: props.map(AnyHashable.init)) { [weak self] index, size in
            let core = CoreTouchArea(NativeTouchArea(props: props[Int(index)], width: size.width, height: size.height))
            return DebugTouchArea(core: core) { self?.fired($0) }
        }
        materials.startResolution(nodeCount: tree.materialsCount, touch: factory)
        let drawn = [layers.card] + (revealed ? [] : [layers.scratch]) + (confirmed ? [] : [layers.slider])
        materials.update(nodes: drawn.map { .ready([$0]) }, noise: noise)
    }

    func fired(_ action: String) {
        logger.info("Debug materials card fired \(action)")
        switch action {
        case "revealed": revealed = true
        case "confirm": confirmed = true
        default: return
        }
        do {
            try render()
        } catch {
            refusal = "\(error)"
        }
    }
}

/// Hands the core's actions to the card as a product's worker would hear them.
private final class DebugTouchArea: WidgetTouchTracking {
    private let core: CoreTouchArea
    private let fire: @MainActor (String) -> Void

    init(core: CoreTouchArea, fire: @escaping @MainActor (String) -> Void) {
        self.core = core
        self.fire = fire
    }

    func touch(_ phase: WidgetTouchPhase, at point: CGPoint, time: TimeInterval) -> [String] {
        let actions = core.touch(phase, at: point, time: time)
        Task { @MainActor [fire] in actions.forEach(fire) }
        return actions
    }

    func inputs(now: TimeInterval) -> WidgetTouchInputs {
        core.inputs(now: now)
    }

    func trail() -> WidgetTrailImage? {
        core.trail()
    }
}

extension DebugMaterialsCardViewModel {
    static func bundledLayer() throws -> WidgetMaterialLayer {
        try bundledLayer(named: "DebugMaterialLayer")
    }

    static func bundledLayer(named name: String) throws -> WidgetMaterialLayer {
        guard let url = Bundle.main.url(forResource: name, withExtension: "json") else {
            throw CocoaError(.fileNoSuchFile)
        }
        return try JSONDecoder().decode(WidgetMaterialLayer.self, from: Data(contentsOf: url))
    }
}

private extension DebugMaterialsCardViewModel {
    static func cardsJson(layers: DebugLayers, revealed: Bool, confirmed: Bool) -> String {
        let scratch = revealed ? text("You won 1 DOT") : scratchJson(material: layers.scratch.material)
        let slider = confirmed ? text("Confirmed") : sliderJson(material: layers.slider.material)
        return """
        {"tag":"Column","value":{"modifiers":[{"tag":"FillWidth","value":true}],
        "props":{"horizontalAlignment":null,"verticalArrangement":null},"children":[
        \(cardJson(material: layers.card.material)),\(scratch),\(slider)]}}
        """
    }

    static func text(_ value: String) -> String {
        """
        {"tag":"Box","value":{"modifiers":[{"tag":"FillWidth","value":true},{"tag":"Height","value":96},
        {"tag":"Padding","value":{"top":24,"bottom":24,"start":24,"end":24}}],"props":{"contentAlignment":null},
        "children":[{"tag":"Text","value":{"modifiers":[],"props":{"style":null,"color":null},
        "children":[{"tag":"String","value":{"text":"\(value)"}}]}}]}}
        """
    }

    static func scratchJson(material: Data) -> String {
        """
        {"tag":"Touch","value":{"props":{"drag":"Any","trail":{"radius":20},
        "triggers":[{"when":{"tag":"TrailCovers","value":600},"action":"revealed"}]},
        "children":[{"tag":"Materials","value":{"props":{"layers":[{"material":"\(material.toHex(includePrefix: true))",
        "args":[{"tag":"SamplerFloat","value":{"tag":"Trail"}}]}]},
        "children":[{"tag":"Box","value":{"modifiers":[{"tag":"Width","value":300},{"tag":"Height","value":200},
        {"tag":"Padding","value":{"top":24,"bottom":24,"start":24,"end":24}}],"props":{"contentAlignment":null},
        "children":[{"tag":"Text","value":{"modifiers":[],"props":{"style":"HeadlineLarge","color":null},
        "children":[{"tag":"String","value":{"text":"You won 1 DOT"}}]}}]}}]}}]}}
        """
    }

    static func sliderJson(material: Data) -> String {
        """
        {"tag":"Touch","value":{"props":{"drag":"Horizontal","trail":null,
        "triggers":[{"when":{"tag":"Drag","value":{"from":{"x":0,"y":0,"width":200,"height":1000},
        "to":{"x":900,"y":0,"width":100,"height":1000}}},"action":"confirm"}]},
        "children":[{"tag":"Materials","value":{"props":{"layers":[{"material":"\(material.toHex(includePrefix: true))",
        "args":[{"tag":"Touch"},{"tag":"TouchState"}]}]},
        "children":[{"tag":"Box","value":{"modifiers":[{"tag":"Width","value":300},{"tag":"Height","value":64},
        {"tag":"Padding","value":{"top":20,"bottom":20,"start":24,"end":24}}],"props":{"contentAlignment":null},
        "children":[{"tag":"Text","value":{"modifiers":[],"props":{"style":null,"color":null},
        "children":[{"tag":"String","value":{"text":"Slide to confirm"}}]}}]}}]}}]}}
        """
    }

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
