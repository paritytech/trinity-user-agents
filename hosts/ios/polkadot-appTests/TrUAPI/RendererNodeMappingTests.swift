import Foundation
import PolkadotUI
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

    @Test func boxNodeMapsItsChildren() {
        let node = RendererNode.box(
            modifiers: [],
            props: BoxProps(contentAlignment: nil),
            children: [.spacer(modifiers: []), .spacer(modifiers: [])]
        )

        let mapped = node.toWidgetNode(resolver: resolver)

        guard case let .box(_, children) = mapped?.content else {
            Issue.record("expected a box node, got \(String(describing: mapped?.content))")
            return
        }
        #expect(children.count == 2)
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

    /// The effect is a shader drawn over whatever is inside it, so the node has
    /// to survive the mapping with its children under it: no node leaves the
    /// shader unapplied, no children leave it nothing to draw over.
    @Test func anEffectKeepsItsChildrenAndItsOwnNode() throws {
        let effect = RendererNode.effect(
            props: EffectProps(effect: .rainbow),
            children: [
                .text(modifiers: [], props: TextProps(style: nil, color: nil), children: [.string(text: "kept")])
            ]
        )

        guard case let .effect(props, children) = try #require(effect.toWidgetNode(resolver: resolver)).content,
              case let .text(textProps) = children.first?.content
        else {
            Issue.record("an effect should keep both its own node and its children")
            return
        }
        #expect(props.effect == .rainbow)
        #expect(textProps.text == "kept")
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
