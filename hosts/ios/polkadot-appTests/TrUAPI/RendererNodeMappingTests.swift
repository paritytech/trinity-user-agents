import Foundation
@testable import PolkadotUI
import Products
import SwiftUI
import Testing
import TrUAPIHost
@testable import polkadot_app

@Suite("RendererNode mapping")
struct RendererNodeMappingTests {
    private let resolver = StubWidgetDesignTokenResolver()

    private func paddingOfBox(_ dimensions: Dimensions) throws -> EdgeInsets {
        let node = RendererNode.box(
            modifiers: [.padding(dimensions)],
            props: BoxProps(contentAlignment: nil),
            children: []
        )
        let widget = try #require(node.toWidgetNode(resolver: resolver))
        return widget.modifiers.padding
    }

    private func opacityOfBox(_ modifiers: [Modifier]) throws -> CGFloat? {
        let node = RendererNode.box(
            modifiers: modifiers,
            props: BoxProps(contentAlignment: nil),
            children: []
        )
        let widget = try #require(node.toWidgetNode(resolver: resolver))
        return widget.modifiers.opacity
    }

    @Test func opacityMapsTheCoreRangeOntoSwiftUI() throws {
        #expect(try opacityOfBox([.opacity(0)]) == 0)
        #expect(try opacityOfBox([.opacity(255)]) == 1)
        #expect(try opacityOfBox([.opacity(128)]) == CGFloat(128) / 255)
    }

    /// No modifier is fully opaque, and the view applies `?? 1` to say so.
    @Test func noOpacityModifierLeavesItUnset() throws {
        #expect(try opacityOfBox([]) == nil)
    }

    @Test func everyEdgeIsExplicit() throws {
        let padding = try paddingOfBox(Dimensions(top: 4, end: 8, bottom: 12, start: 16))

        #expect(padding.top == 4)
        #expect(padding.trailing == 8)
        #expect(padding.bottom == 12)
        #expect(padding.leading == 16)
    }

    @Test func bottomDefaultsToTopAndStartToEnd() throws {
        let padding = try paddingOfBox(Dimensions(top: 4, end: 8, bottom: nil, start: nil))

        #expect(padding.top == 4)
        #expect(padding.trailing == 8)
        #expect(padding.bottom == 4)
        #expect(padding.leading == 8)
    }

    @Test func uniformDimensionsAreUnchanged() throws {
        let padding = try paddingOfBox(Dimensions(top: 16, end: 16, bottom: 16, start: 16))

        #expect(padding.top == 16)
        #expect(padding.leading == 16)
        #expect(padding.bottom == 16)
        #expect(padding.trailing == 16)
    }

    /// A size crosses the wire as an unsigned 64-bit number. Converting one
    /// straight to `CGFloat` hands SwiftUI a value no layout can satisfy, so a
    /// product could take the card's screen down by naming a big enough number.
    /// It is drawn at a bound instead: visibly wrong, still drawing.
    @Test func sizeBeyondAnyScreenIsClampedRatherThanRefused() {
        let node = RendererNode.spacer(modifiers: [.width(UInt64.max)])

        let mapped = node.toWidgetNode(resolver: resolver)

        let width = try? #require(mapped?.modifiers.width)
        #expect(width == 100_000)
    }

    /// `fillWidth(false)` is a product saying "do not fill", which must not
    /// read the same as asking to fill.
    @Test func fillWidthAppliesOnlyWhenAskedFor() {
        let filling = RendererNode.spacer(modifiers: [.fillWidth(true)])
        let notFilling = RendererNode.spacer(modifiers: [.fillWidth(false)])

        #expect(filling.toWidgetNode(resolver: resolver)?.modifiers.fillWidth == true)
        #expect(notFilling.toWidgetNode(resolver: resolver)?.modifiers.fillWidth == false)
    }

    @Test func blendingModeIsCarried() {
        let node = RendererNode.spacer(modifiers: [.blendingMode(.multiply)])

        let mapped = node.toWidgetNode(resolver: resolver)

        #expect(mapped?.modifiers.blendingMode == .multiply)
    }

    @Test func textNodeAbsorbsItsStringChildren() {
        let node = RendererNode.text(
            modifiers: [],
            props: TextProps(style: nil, color: nil),
            children: [.string(text: "Loy"), .string(text: "alty")]
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .text(props) = mapped?.content else {
            Issue.record("expected a text node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.text == "Loyalty")
    }

    @Test func textNodeUsesTheColorTokenItNames() {
        let tokens = WidgetDesignTokenResolver()
        let node = RendererNode.text(
            modifiers: [],
            props: TextProps(style: nil, color: .fgError),
            children: [.string(text: "Expired")]
        )

        let mapped = node.toWidgetNode(resolver: tokens)

        guard case let .text(props) = mapped?.content else {
            Issue.record("expected a text node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.color.rgba == tokens.color(for: .error).rgba)
        // Guards the assertion above: the resolved values must also tell the
        // named token apart from the default the mapper falls back to.
        #expect(props.color.rgba != tokens.color(for: .textPrimary).rgba)
    }

    @Test func textNodeUsesTheTypographyStyleItNames() {
        let tokens = WidgetDesignTokenResolver()
        let node = RendererNode.text(
            modifiers: [],
            props: TextProps(style: .bodySmallRegular, color: nil),
            children: [.string(text: "Fine print")]
        )

        let mapped = node.toWidgetNode(resolver: tokens)

        guard case let .text(props) = mapped?.content else {
            Issue.record("expected a text node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.labelStyle.font == PolkadotUI.LabelStyle.caption12Regular().font)
    }

    /// A box that names no alignment places its children at the top start.
    @Test func boxNodeMapsItsChildren() {
        let node = RendererNode.box(
            modifiers: [],
            props: BoxProps(contentAlignment: nil),
            children: [.spacer(modifiers: []), .spacer(modifiers: [])]
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .box(props, children) = mapped?.content else {
            Issue.record("expected a box node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(children.count == 2)
        #expect(props.alignment == .topLeading)
    }

    /// A `String` run is absorbed as its parent's text and `Nil` draws nothing,
    /// so neither becomes a child view of its own.
    @Test func containerDropsNilAndStringRuns() {
        let node = RendererNode.column(
            modifiers: [],
            props: ColumnProps(horizontalAlignment: nil, verticalArrangement: nil),
            children: [.nil, .string(text: "loose"), .spacer(modifiers: [])]
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .column(_, children) = mapped?.content else {
            Issue.record("expected a column node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(children.count == 1)
    }

    @Test func rowNodeCarriesItsAlignmentAndArrangement() {
        let node = RendererNode.row(
            modifiers: [],
            props: RowProps(verticalAlignment: .bottom, horizontalArrangement: .spaceBetween),
            children: []
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .row(props, _) = mapped?.content else {
            Issue.record("expected a row node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.alignment == .bottom)
        #expect(props.arrangement == .spaceBetween)
    }

    @Test func buttonNodeCarriesItsActionAndVariant() {
        let node = RendererNode.button(
            modifiers: [],
            props: ButtonProps(
                text: "Stamp",
                variant: .secondary,
                enabled: nil,
                loading: nil,
                clickAction: "stamp"
            ),
            children: []
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .button(props) = mapped?.content else {
            Issue.record("expected a button node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.text == "Stamp")
        #expect(props.variant == .secondary)
        #expect(props.clickAction == "stamp")
    }

    /// A product that says nothing about a button's state gets one that is
    /// pressable and not spinning, so an omitted field is never read as `false`.
    @Test func buttonNodeDefaultsToEnabledAndNotLoading() {
        let node = RendererNode.button(
            modifiers: [],
            props: ButtonProps(text: "Stamp", variant: nil, enabled: nil, loading: nil, clickAction: nil),
            children: []
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .button(props) = mapped?.content else {
            Issue.record("expected a button node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.isEnabled)
        #expect(!props.isLoading)
    }

    @Test func textFieldNodeCarriesItsValueAndChangeAction() {
        let node = RendererNode.textField(
            modifiers: [],
            props: TextFieldProps(
                text: "half",
                placeholder: "amount",
                label: "Amount",
                enabled: false,
                valueChangeAction: "edit"
            )
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .textField(props) = mapped?.content else {
            Issue.record("expected a text field node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.text == "half")
        #expect(props.placeholder == "amount")
        #expect(props.label == "Amount")
        #expect(props.valueChangeAction == "edit")
        #expect(!props.isEnabled)
    }

    /// The picture is fetched after its node is drawn, so the node has to carry
    /// the source that fetch reads, and the space its modifiers reserve has to
    /// hold before the bytes arrive, or everything around it moves when they do.
    @Test func anImageDrawsItsSourceAndKeepsItsSpace() throws {
        let image = RendererNode.image(
            modifiers: [.width(120), .height(80)],
            props: ImageProps(source: .bulletin("bafyimage"), fit: nil)
        )

        let widget = try #require(image.toWidgetNode(resolver: resolver))

        guard case let .image(props) = widget.content else {
            Issue.record("an image node should draw its source")
            return
        }
        #expect(props.source == .bulletin(cid: "bafyimage"))
        #expect(widget.modifiers.width == 120)
        #expect(widget.modifiers.height == 80)
    }

    /// A picture the host holds in an archive is named by path rather than by
    /// content, and the fit is what makes one that does not match its box
    /// usable, so both have to survive the mapping.
    @Test func imageNodeCarriesItsSourceAndFit() {
        let node = RendererNode.image(
            modifiers: [],
            props: ImageProps(source: .archive("faces/logo.png"), fit: .contain)
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .image(props) = mapped?.content else {
            Issue.record("expected an image node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.source == .archive(path: "faces/logo.png"))
        #expect(props.fit == .contain)
    }

    /// An omitted fit must not read as `.none`, which leaves the picture at
    /// whatever size it happens to be instead of the size the product asked for.
    @Test func imageNodeDefaultsToFillWhenNoFitIsNamed() {
        let node = RendererNode.image(
            modifiers: [],
            props: ImageProps(source: .bulletin("bafyreih"), fit: nil)
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .image(props) = mapped?.content else {
            Issue.record("expected an image node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(props.fit == .fill)
    }

    /// The renderer gained a square shape; the resolver is defined over
    /// `ScaleShape`, which spells one as a zero-radius rounded rect. Asserted
    /// on what the resolver was handed: the stub ignores its argument, so a
    /// wrong translation would still produce a border.
    @Test func squareTranslatesToAZeroRadiusRoundedRect() throws {
        let recorder = ShapeRecordingResolver()
        let node = RendererNode.box(
            modifiers: [.border(BorderStyle(width: 2, color: .fgPrimary, shape: .square))],
            props: BoxProps(contentAlignment: nil),
            children: []
        )

        _ = try #require(node.toWidgetNode(resolver: recorder))

        guard case let .rounded(radius) = try #require(recorder.shapes.first) else {
            Issue.record("a square must translate to a rounded rect, not a circle")
            return
        }
        #expect(radius == 0)
    }

    @Test func materialsKeepTheirChildren() throws {
        // Given
        let materials = RendererNode.materials(
            props: MaterialsProps(layers: [], bleed: 0),
            children: [
                .text(modifiers: [], props: TextProps(style: nil, color: nil), children: [.string(text: "kept")])
            ]
        )

        // When
        let node = try #require(materials.toWidgetNode(resolver: resolver))

        // Then
        guard case let .materials(index, _, children) = node.content,
              case let .text(textProps) = children.first?.content
        else {
            Issue.record("a materials node should keep its children")
            return
        }
        #expect(index == 0)
        #expect(textProps.text == "kept")
    }

    /// The core keeps one state per `Touch` node, depth first, so a node drawn under another index
    /// would scratch another node's trail. A node past a limit draws its children like a box.
    @Test func touchNodesAreNumberedDepthFirstAndCarryTheCoreVerdict() throws {
        // Given
        func touch(_ drag: DragAxis, _ children: [RendererNode]) -> RendererNode {
            let tap = TouchTrigger(when: .tap(Region(x: 0, y: 0, width: 1_000, height: 1_000)), action: "flip")
            return .touch(props: TouchProps(drag: drag, trail: nil, triggers: [tap], replay: nil), children: children)
        }
        let materials = RendererNode.materials(props: MaterialsProps(layers: [], bleed: 0), children: [])
        let tree = RendererNode.box(
            modifiers: [],
            props: BoxProps(contentAlignment: nil),
            children: [touch(.any, [materials]), touch(.horizontal, [touch(.any, [])])]
        )

        // When
        let node = try #require(tree.toWidgetNode(resolver: resolver))

        // Then
        guard case let .box(_, children) = node.content,
              case let .touch(first, firstChildren) = children.first?.content,
              case let .touch(second, secondChildren) = children.last?.content,
              case let .touch(nested, _) = secondChildren.first?.content,
              case let .materials(materialsIndex, _, _) = firstChildren.first?.content
        else {
            Issue.record("each touch node should keep its children")
            return
        }
        #expect(first == WidgetTouchNode(index: 0, drag: .any, takesTouch: true, accessibilityActions: ["flip"]))
        #expect(second == WidgetTouchNode(
            index: 1,
            drag: .horizontal,
            takesTouch: true,
            accessibilityActions: ["flip"]
        ))
        #expect(nested == WidgetTouchNode(index: 2, drag: .any, takesTouch: false, accessibilityActions: ["flip"]))
        #expect(materialsIndex == 0)
    }

    /// The core answers one state per `Materials` node, depth first, so a node drawn under another
    /// index would draw the layers of a different node. A node hidden inside a text still counts.
    @Test func materialsAreNumberedDepthFirstLikeTheCore() throws {
        // Given
        func materials(_ children: [RendererNode]) -> RendererNode {
            .materials(props: MaterialsProps(layers: [], bleed: 0), children: children)
        }
        let text = RendererNode.text(
            modifiers: [],
            props: TextProps(style: nil, color: nil),
            children: [materials([])]
        )
        let tree = RendererNode.column(
            modifiers: [],
            props: ColumnProps(horizontalAlignment: nil, verticalArrangement: nil),
            children: [materials([materials([])]), text, materials([])]
        )

        // When
        let column = try #require(tree.toWidgetNode(resolver: resolver))

        // Then
        #expect(materialsIndices(column) == [0, 1, 3])
    }

    /// Text and button children draw no widget, but the core walks them to any depth.
    @Test func materialsDeepInsideATextOrAButtonStillShiftTheNextIndex() throws {
        // Given
        func materials(_ children: [RendererNode]) -> RendererNode {
            .materials(props: MaterialsProps(layers: [], bleed: 0), children: children)
        }
        let boxed = RendererNode.box(modifiers: [], props: BoxProps(contentAlignment: nil), children: [materials([])])
        let text = RendererNode.text(modifiers: [], props: TextProps(style: nil, color: nil), children: [boxed])
        let button = RendererNode.button(
            modifiers: [],
            props: ButtonProps(text: "Stamp", variant: nil, enabled: nil, loading: nil, clickAction: nil),
            children: [materials([])]
        )
        let tree = RendererNode.column(
            modifiers: [],
            props: ColumnProps(horizontalAlignment: nil, verticalArrangement: nil),
            children: [text, button, materials([])]
        )

        // When
        let column = try #require(tree.toWidgetNode(resolver: resolver))

        // Then
        #expect(materialsIndices(column) == [2])
        #expect(tree.nodeMaterials.count == 3)
    }

    /// A new tree draws the last output of a node with the same materials, so each node names its
    /// layers in the order the core resolves the nodes.
    @Test func nodeMaterialsNameEachLayerDepthFirst() {
        // Given
        func materials(_ ids: [UInt8], _ children: [RendererNode]) -> RendererNode {
            let layers = ids.map { MaterialInstance(material: Data(repeating: $0, count: 32), args: []) }
            return .materials(props: MaterialsProps(layers: layers, bleed: 0), children: children)
        }
        let tree = RendererNode.column(
            modifiers: [],
            props: ColumnProps(horizontalAlignment: nil, verticalArrangement: nil),
            children: [materials([1, 2], [materials([3], [])]), materials([], [])]
        )

        // Then
        let expected = [[1, 2], [3], []].map { $0.map { Data(repeating: $0, count: 32) } }
        #expect(tree.nodeMaterials == expected)
    }

    @Test func aMaterialsNodeCarriesItsBleedInPoints() throws {
        // Given
        let materials = RendererNode.materials(props: MaterialsProps(layers: [], bleed: 48), children: [])

        // When
        let node = try #require(materials.toWidgetNode(resolver: resolver))

        // Then
        guard case let .materials(_, bleed, _) = node.content else {
            Issue.record("a materials node should stay one")
            return
        }
        #expect(bleed == 48)
    }

    /// The design's action label is weight 500, which a regular face thickened by the system does not match.
    @Test func mediumBodyTextDrawsInTheMediumFace() throws {
        // Given
        let text = RendererNode.text(
            modifiers: [],
            props: TextProps(style: .bodyMediumMedium, color: nil),
            children: [.string(text: "Set up your member name")]
        )

        // When
        let node = try #require(text.toWidgetNode(resolver: resolver))

        // Then
        guard case let .text(props) = node.content else {
            Issue.record("a text node should stay one")
            return
        }
        let font = try #require(props.labelStyle.font)
        #expect([font.fontName, "\(font.pointSize)"] == [UIFont.body14Medium().fontName, "14.0"])
        #expect(font.fontName != UIFont.body14Regular().fontName)
        #expect(props.labelStyle.lineHeight == 20)
    }

    private func materialsIndices(_ node: CustomMessageWidgetNode) -> [UInt32] {
        switch node.content {
        case let .materials(index, _, children):
            [index] + children.flatMap(materialsIndices)
        case let .box(_, children),
             let .column(_, children),
             let .row(_, children),
             let .touch(_, children):
            children.flatMap(materialsIndices)
        case .spacer,
             .text,
             .button,
             .textField,
             .image:
            []
        }
    }
}

/// Records the shapes the mapping resolves, so a token translation can be
/// asserted instead of merely exercised.
private final class ShapeRecordingResolver: WidgetDesignTokenResolving {
    private(set) var shapes: [ScaleShape] = []

    func color(for _: ScaleColorToken) -> Color { .clear }
    func font(for _: ScaleTypographyStyle) -> Font { .body }
    func labelStyle(for _: ScaleTypographyStyle) -> (font: Font, lineSpacing: CGFloat) { (.body, 0) }
    func cornerRadius(for _: ScaleShape) -> CGFloat { 0 }
    func buttonStyle(for _: ScaleButtonVariant) -> (background: Color, foreground: Color) { (.clear, .clear) }

    func shape(for scaleShape: ScaleShape) -> AnyShape {
        shapes.append(scaleShape)
        return AnyShape(Rectangle())
    }
}

private extension Color {
    /// SwiftUI `Color` is not reliably `Equatable` for asset-backed colors: two
    /// instances of the same design-system color compare unequal. Tests compare
    /// the concrete values each resolves to instead.
    var rgba: [CGFloat]? {
        let resolved = UIColor(self).resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark))
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        guard resolved.getRed(&red, green: &green, blue: &blue, alpha: &alpha) else { return nil }
        return [red, green, blue, alpha]
    }
}

private extension PolkadotUI.LabelStyle {
    /// `LabelStyle` is not `Equatable` and its fields are internal to PolkadotUI,
    /// so the font it carries is read back through its public attributes.
    var font: UIFont? {
        attributes()[.font] as? UIFont
    }
}
