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

    @Test func aMaskModifierCarriesItsShapesInPoints() throws {
        let nib = MaskBrush(tip: .capsule, aspectPermille: 350, angleCentideg: -3_000, spacingPermille: 250)
        let tip = CustomMessageWidgetNode.Mask.Brush(tip: .capsule, aspect: 0.35, angle: -30 * .pi / 180, spacing: 0.25)
        let mask = Mask(
            key: "veil",
            mode: .hide,
            shapes: [
                .all,
                .circle(center: MaskPoint(x: 1_050, y: -200), radius: 2_600),
                .stroke(width: 5_200, points: [MaskPoint(x: 0, y: 0), MaskPoint(x: 100, y: 250)]),
                .stamped(width: 4_000, brush: nib, points: [MaskPoint(x: 200, y: 300)]),
            ],
            follow: MaskFollow(pan: "pad", width: 5_200, brush: nib),
            fadeMs: 350,
            clearedShare: ClearedShare(
                atPermille: 550, action: "done", whileDown: true,
                rescue: ShareRescue(afterMs: 1_500, fromPermille: 450)
            ),
            haptics: MaskHaptics(everyPermille: 100, step: .tick, reach: .success)
        )
        let node = RendererNode.image(
            modifiers: [.width(290), .mask(mask)],
            props: ImageProps(source: .archive("veil.png"), fit: .fill)
        )
        let widget = try #require(node.toWidgetNode(resolver: resolver))
        let mapped = try #require(widget.modifiers.mask)
        #expect(mapped.key == "veil" && mapped.mode == .hide)
        #expect(mapped.shapes == [
            .all,
            .circle(center: CGPoint(x: 10.5, y: -2), radius: 26),
            .stroke(width: 52, points: [.zero, CGPoint(x: 1, y: 2.5)]),
            .stamped(width: 40, brush: tip, points: [CGPoint(x: 2, y: 3)]),
        ])
        #expect(mapped.follow == .init(pan: "pad", width: 52, brush: tip))
        #expect(abs(mapped.fade - 0.35) < 0.0001)
        #expect(mapped.clearedShare == .init(
            atPermille: 550, action: "done", whileDown: true, rescue: .init(after: 1.5, fromPermille: 450)
        ))
        #expect(mapped.haptics == .init(everyPermille: 100, step: .tick, reach: .success))
    }

    @Test func aShowMaskWithNoKeyMapsAsSent() throws {
        let node = RendererNode.box(
            modifiers: [.mask(Mask(key: nil, mode: .show, shapes: [], follow: nil, fadeMs: nil, clearedShare: nil))],
            props: BoxProps(contentAlignment: nil),
            children: []
        )
        let mapped = try #require(node.toWidgetNode(resolver: resolver)?.modifiers.mask)
        #expect(mapped == CustomMessageWidgetNode.Mask(key: nil, mode: .show))
    }

    @Test func maskUpdatesInsideATreeDrawNothing() {
        let node = RendererNode.box(
            modifiers: [],
            props: BoxProps(contentAlignment: nil),
            children: [.extendMask(key: "veil", shapes: [.all])]
        )
        guard case let .box(_, children) = node.toWidgetNode(resolver: resolver)?.content else {
            Issue.record("the box did not map")
            return
        }
        #expect(children.isEmpty)
    }

    @Test func aLocationPanReportsWhereTheFingerIs() throws {
        let pan = PanGesture(
            axis: .free,
            drag: nil,
            startAction: "start",
            moveAction: "move",
            moveIntervalMs: nil,
            endAction: "end",
            location: true
        )
        let node = RendererNode.gesture(
            modifiers: [],
            props: GestureProps(
                tapAction: nil, pan: pan, accessibilityLabel: nil, enabled: nil, hapticOnComplete: nil, key: "pad",
                feel: nil
            ),
            children: []
        )
        guard case let .gesture(props, _) = try #require(node.toWidgetNode(resolver: resolver)).content else {
            Issue.record("the gesture did not map")
            return
        }
        #expect(props.pan?.reportsLocation == true)
        #expect(props.pan?.axis == .free)
        #expect(props.key == "pad")
    }

    @Test func enterAndTracksMapInPointsDegreesAndSeconds() throws {
        let linear = Curve(x1: 0, y1: 0, x2: 1_000, y2: 1_000)
        let enter = Enter(
            from: Pose(opacityPermille: 0, scalePermille: 900, x: 0, y: 1_200, rotationCentideg: -500),
            motion: .spring(Spring(omegaCenti: 1_800, dampingCenti: 80)),
            delayMs: 50
        )
        let pulse = Keyframes(
            frames: [Keyframe(atMs: 0, value: 0, curve: linear), Keyframe(atMs: 100, value: 1_000, curve: linear)],
            repeat: 0,
            alternate: true
        )
        let node = RendererNode.box(
            modifiers: [
                .enter(enter),
                .animate(Track(property: .scale, play: .keyframes(pulse))),
                .animate(Track(
                    property: .offsetX,
                    play: .follow(target: -400, motion: .timed(ms: 250, curve: linear))
                )),
                .animate(Track(property: .scale, play: .follow(target: 1_500, motion: .timed(ms: 250, curve: linear)))),
            ],
            props: BoxProps(contentAlignment: nil),
            children: []
        )
        let modifiers = try #require(node.toWidgetNode(resolver: resolver)).modifiers
        #expect(modifiers.enter == .init(
            from: .init(opacity: 0, scale: 0.9, offset: CGSize(width: 0, height: 12), rotation: -5),
            motion: .spring(omega: 18, damping: 0.8),
            delay: 0.05
        ))
        let timed = CustomMessageWidgetNode.Motion.timed(duration: 0.25, curve: .linear)
        #expect(modifiers.tracks == [
            .init(property: .offsetX, play: .follow(target: -4, motion: timed)),
            .init(property: .scale, play: .follow(target: 1.5, motion: timed)),
        ], "a node keeps its last track per property")
        let keyframes = try #require(
            RendererNode.box(
                modifiers: [.animate(Track(property: .rotation, play: .keyframes(pulse)))],
                props: BoxProps(contentAlignment: nil),
                children: []
            ).toWidgetNode(resolver: resolver)?.modifiers.tracks.first
        )
        #expect(keyframes == .init(property: .rotation, play: .keyframes(.init(
            frames: [.init(at: 0, value: 0), .init(at: 0.1, value: 10)], runs: 0, alternate: true
        ))))
    }

    @Test func anEmitterMapsInPointsRadiansAndSecondsAndTheLastIsKept() throws {
        let sparks = Emitter(
            looks: [
                .circle(diameter: 300, color: 0xFF00_00FF),
                .rectangle(width: 400, height: 200, color: 0x00FF_00FF),
                .image(source: .archive("particle.png"), width: 1_200, height: 800),
                .image(source: .inline(Data([1])), width: 100, height: 100),
            ],
            source: .point(MaskPoint(x: 500, y: -200)),
            ratePerS: 12,
            ratePer100pt: 3,
            lifeMs: 1_500,
            speed: ValueRange(min: 12_000, max: 4_000),
            angleCentideg: 9_000,
            spreadCentideg: 3_000,
            gravity: 98_000,
            spin: ValueRange(min: -18_000, max: 18_000),
            fadeFromPermille: 600,
            endScalePermille: 200,
            maxAlive: 80,
            bursts: [
                Burst(count: 20, on: .action("done")),
                Burst(count: 5, on: .change(3)),
                Burst(count: 1, on: .appear)
            ]
        )
        var first = sparks
        first.source = .area
        let node = RendererNode.box(
            modifiers: [.emit(first), .emit(sparks)],
            props: BoxProps(contentAlignment: nil),
            children: []
        )
        let emitter = try #require(node.toWidgetNode(resolver: resolver)?.modifiers.emitter)
        #expect(emitter.source == .point(CGPoint(x: 5, y: -2)), "a node keeps its last emitter")
        #expect(emitter.looks.map(\.size) == [
            CGSize(width: 3, height: 3), CGSize(width: 4, height: 2), CGSize(width: 12, height: 8),
            CGSize(width: 1, height: 1),
        ])
        guard case let .image(fetched, _) = emitter.looks[2], case let .image(refused, _) = emitter.looks[3] else {
            Issue.record("the images did not map")
            return
        }
        #expect(fetched == .archive(path: "particle.png") && refused == nil, "an inline picture is refused")
        #expect(emitter.ratePerSecond == 12 && emitter.ratePer100Points == 3 && emitter.life == 1.5)
        #expect(emitter.speed == 40 ... 120, "the ends are put in order")
        #expect(abs(emitter.angle - .pi / 2) < 1e-9 && abs(emitter.spread - .pi / 6) < 1e-9)
        #expect(emitter.gravity == 980 && emitter.spin == -180 ... 180)
        #expect(emitter.fadeFrom == 0.6 && emitter.endScale == 0.2 && emitter.maxAlive == 80)
        #expect(emitter.bursts == [
            .init(count: 20, on: .action("done")), .init(count: 5, on: .change(3)), .init(count: 1, on: .appear),
        ])
    }

    @Test func parallaxMapsInPointsDegreesAndSeconds() throws {
        let node = RendererNode.box(
            modifiers: [
                .parallax(Parallax(depthPermille: 1_000, tilt: nil, pan: nil)),
                .parallax(Parallax(
                    depthPermille: -300,
                    tilt: TiltParallax(travel: 2_400, maxTiltCentideg: 2_000, settleMs: 3_000),
                    pan: "card"
                )),
            ],
            props: BoxProps(contentAlignment: nil),
            children: []
        )
        let parallax = try #require(node.toWidgetNode(resolver: resolver)?.modifiers.parallax)
        #expect(parallax == .init(depth: -0.3, tilt: .init(travel: 24, maxTilt: 20, settle: 3), pan: "card"))
    }

    @Test func aPressFeelScalesTheAxesApart() throws {
        let feel = { (down: UInt16?) in
            PanFeel(
                pressScalePermille: 1_040, pressMs: 120, unpressMs: 200, springOmegaCenti: 1_800,
                springDampingCenti: 80, approachMs: 300, approachReference: 100, popPermille: [], popOffset: 0,
                pressScaleYPermille: down
            )
        }
        let mapped = try [feel(960), feel(nil)].map { feel in
            let node = RendererNode.gesture(
                modifiers: [],
                props: GestureProps(
                    tapAction: nil, pan: nil, accessibilityLabel: nil, enabled: nil, hapticOnComplete: nil, key: nil,
                    feel: feel
                ),
                children: []
            )
            guard case let .gesture(props, _) = try #require(node.toWidgetNode(resolver: resolver)).content else {
                throw MappingFailure()
            }
            return try #require(props.feel)
        }
        #expect(abs(mapped[0].pressScale - 1.04) < 0.0001)
        #expect(abs((mapped[0].pressScaleY ?? 0) - 0.96) < 0.0001)
        #expect(mapped[1].pressScaleY == nil, "no y scale presses both axes by pressScale")
    }

    @Test func materialsDrawTheirChildrenPlain() throws {
        let materials = RendererNode.materials(
            props: MaterialsProps(layers: []),
            children: [
                .text(modifiers: [], props: TextProps(style: nil, color: nil), children: [.string(text: "kept")])
            ]
        )

        guard case let .box(props, children) = try #require(materials.toWidgetNode(resolver: resolver)).content,
              case let .text(textProps) = children.first?.content
        else {
            Issue.record("a materials node should draw as a box around its children")
            return
        }
        #expect(props.alignment == .topLeading)
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

private struct MappingFailure: Error {}
