import PolkadotUI
import Products
import SwiftUI
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The field names and defaults follow the Android prototype's mapping, so a
/// product's card reads the same on both hosts.
@Suite("SlideToConfirm mapping")
struct SlideToConfirmMappingTests {
    private let resolver = StubWidgetDesignTokenResolver()

    private func slideProps(
        of node: RendererNode,
        resolver: any WidgetDesignTokenResolving = StubWidgetDesignTokenResolver()
    ) throws -> CustomMessageWidgetNode.SlideToConfirmProps {
        let mapped = try #require(node.toWidgetNode(resolver: resolver))
        guard case let .slideToConfirm(props) = mapped.content else {
            Issue.record("expected a slide-to-confirm node, got \(mapped.content)")
            throw MappingFailure.wrongNode
        }
        return props
    }

    @Test("Every field the product sets reaches the control")
    func carriesEveryField() throws {
        let node = RendererNode.slideToConfirm(
            modifiers: [],
            props: SlideToConfirmProps(
                text: "Slide to register",
                secondaryText: "Friday 20:00",
                confirmedText: "I'll be there",
                variant: .reveal,
                accent: nil,
                enabled: false,
                loading: true,
                confirmed: true,
                confirmAction: "register"
            )
        )

        let props = try slideProps(of: node)

        #expect(props.text == "Slide to register")
        #expect(props.secondaryText == "Friday 20:00")
        #expect(props.confirmedText == "I'll be there")
        #expect(props.variant == .reveal)
        #expect(props.accent == nil)
        #expect(props.isEnabled == false)
        #expect(props.isLoading == true)
        #expect(props.isConfirmed == true)
        #expect(props.confirmAction == "register")
    }

    @Test("Absent fields take Android's defaults: a thumb, enabled, at rest")
    func absentFieldsTakeTheDefaults() throws {
        let node = RendererNode.slideToConfirm(
            modifiers: [],
            props: SlideToConfirmProps(
                text: "Slide",
                secondaryText: nil,
                confirmedText: nil,
                variant: nil,
                accent: nil,
                enabled: nil,
                loading: nil,
                confirmed: nil,
                confirmAction: "go"
            )
        )

        let props = try slideProps(of: node)

        #expect(props.variant == .thumb)
        #expect(props.isEnabled)
        #expect(!props.isLoading)
        #expect(!props.isConfirmed)
        #expect(props.acceptsSlide)
        #expect(props.displayedText == "Slide")
    }

    @Test("A control without a confirm action draws but is inert")
    func noActionIsInert() throws {
        let node = RendererNode.slideToConfirm(modifiers: [], props: .minimal(confirmAction: nil))

        let props = try slideProps(of: node)

        #expect(props.confirmAction == nil)
        #expect(!props.acceptsSlide)
    }

    @Test("The accent is resolved from the colour token it names")
    func accentUsesItsToken() throws {
        let tokens = WidgetDesignTokenResolver()
        var minimal = SlideToConfirmProps.minimal(confirmAction: "go")
        minimal.accent = .fgSuccess
        let node = RendererNode.slideToConfirm(modifiers: [], props: minimal)

        let accent = try #require(slideProps(of: node, resolver: tokens).accent)

        #expect(accent.darkRGBA == tokens.color(for: .success).darkRGBA)
        #expect(accent.darkRGBA != tokens.color(for: .textPrimary).darkRGBA)
    }

    @Test("Layout modifiers are carried like any other leaf's")
    func modifiersAreCarried() throws {
        let node = RendererNode.slideToConfirm(
            modifiers: [.fillWidth(true), .height(80)],
            props: .minimal(confirmAction: "go")
        )

        let mapped = try #require(node.toWidgetNode(resolver: resolver))

        #expect(mapped.modifiers.fillWidth)
        #expect(mapped.modifiers.height == 80)
    }

    @Test("Confirmed text replaces the label only once confirmed")
    func confirmedTextShowsOnceConfirmed() throws {
        var props = SlideToConfirmProps.minimal(confirmAction: "go")
        props.confirmedText = "Done"
        let atRest = try slideProps(of: .slideToConfirm(modifiers: [], props: props))
        props.confirmed = true
        let confirmed = try slideProps(of: .slideToConfirm(modifiers: [], props: props))

        #expect(atRest.displayedText == "Slide")
        #expect(confirmed.displayedText == "Done")
        #expect(confirmed.isHeldAtEnd)
    }
}

private enum MappingFailure: Error {
    case wrongNode
}

private extension SlideToConfirmProps {
    static func minimal(confirmAction: String?) -> SlideToConfirmProps {
        SlideToConfirmProps(
            text: "Slide",
            secondaryText: nil,
            confirmedText: nil,
            variant: nil,
            accent: nil,
            enabled: nil,
            loading: nil,
            confirmed: nil,
            confirmAction: confirmAction
        )
    }
}

private extension Color {
    /// Asset-backed colours do not compare equal as `Color`, so tests compare
    /// what each resolves to in the app's enforced dark appearance.
    var darkRGBA: [CGFloat]? {
        let resolved = UIColor(self).resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark))
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        guard resolved.getRed(&red, green: &green, blue: &blue, alpha: &alpha) else { return nil }
        return [red, green, blue, alpha]
    }
}
