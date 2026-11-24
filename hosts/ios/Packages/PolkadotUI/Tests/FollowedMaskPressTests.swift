import SwiftUI
import Testing
import UIKit
@testable import PolkadotUI

/// A mask that follows a location pan, laid out in a window: the gesture surface is driven through its own callbacks,
/// as a finger would, and the mask's channel says what it draws.
@MainActor
struct FollowedMaskPressTests {
    @Test(
        "A followed stroke is drawn under the finger while the pan's press scales the masked node",
        arguments: [1, 1.005, 1.2, 0.8] as [CGFloat]
    )
    func strokeStaysUnderTheFinger(pressScale: CGFloat) async throws {
        let scene = try MaskScene(pressScale: pressScale)
        try await scene.stroke(at: CGPoint(x: 150, y: 50))
        let shapes = try scene.drawnShapes()

        // The node scales about its centre (100, 50), so the finger 50 to its right is 50 / scale into the node.
        let expected = CGPoint(x: 100 + 50 / pressScale, y: 50)
        guard case let .stroke(width, points) = try #require(shapes.first), let point = points.first else {
            Issue.record("expected one followed stroke, got \(shapes)")
            return
        }
        #expect(width == 20)
        #expect(abs(point.x - expected.x) < 0.01 && abs(point.y - expected.y) < 0.01, "drawn at \(point)")
    }

    @Test("A press that scales the axes apart keeps the followed stroke under the finger on both")
    func strokeStaysUnderTheFingerWithAxesApart() async throws {
        let scene = try MaskScene(pressScale: 1.2, pressScaleY: 0.8)
        try await scene.stroke(at: CGPoint(x: 150, y: 70))
        let shapes = try scene.drawnShapes()

        // About the centre (100, 50): 50 across at 1.2 and 20 down at 0.8.
        let expected = CGPoint(x: 100 + 50 / 1.2, y: 50 + 20 / 0.8)
        guard case let .stroke(_, points) = try #require(shapes.first), let point = points.first else {
            Issue.record("expected one followed stroke, got \(shapes)")
            return
        }
        #expect(abs(point.x - expected.x) < 0.01 && abs(point.y - expected.y) < 0.01, "drawn at \(point)")
    }
}

/// A 200 by 100 pan over a red node of the same size whose mask hides it under the pan's strokes, 20 wide.
@MainActor
private final class MaskScene {
    let offsets = GestureOffsets()
    private let window: UIWindow
    private let host: UIViewController

    init(pressScale: CGFloat, pressScaleY: CGFloat? = nil) throws {
        var props = CustomMessageWidgetNode.GestureProps { _, _, _ in Data() }
        props.key = "pan"
        var pan = CustomMessageWidgetNode.PanGesture()
        pan.axis = .free
        pan.reportsLocation = true
        props.pan = pan
        if pressScale != 1 {
            var feel = CustomMessageWidgetNode.PanFeel()
            feel.pressScale = pressScale
            feel.pressScaleY = pressScaleY
            props.feel = feel
        }
        let mask = CustomMessageWidgetNode.Mask(key: "veil", follow: .init(pan: "pan", width: 20))
        let masked = CustomMessageWidgetNode(
            content: .box(.init(), children: []),
            modifiers: .init(background: .init(color: .red), width: 200, height: 100, mask: mask)
        )
        let root = CustomMessageWidgetNode(
            content: .gesture(props, children: [masked]),
            modifiers: .init(width: 200, height: 100)
        )
        host = UIHostingController(
            rootView: CustomMessageWidgetView(node: root, resolveImage: nil)
                .environment(\.gestureOffsets, offsets)
                .frame(width: 200, height: 100)
        )
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 100))
        window.backgroundColor = .white
        host.view.backgroundColor = .white
        window.rootViewController = host
        window.isHidden = false
        host.view.frame = window.bounds
        host.view.layoutIfNeeded()
    }

    var surface: GestureSurfaceView {
        get throws {
            try #require(Self.find(GestureSurfaceView.self, in: host.view))
        }
    }

    /// A finger down at `point` (window points), then a short move around it.
    func stroke(at point: CGPoint) async throws {
        await settle()
        let surface = try surface
        surface.onPress?(true, point)
        await settle()
        surface.onPan?(.started, PanTranslation(recognized: .zero, slop: .zero, location: point, touchDown: point))
        surface.onPan?(.moved, PanTranslation(recognized: .zero, slop: .zero, location: point, touchDown: point))
        await settle()
    }

    /// What the keyed mask draws now, in the masked node's own frame.
    func drawnShapes() throws -> [CustomMessageWidgetNode.Mask.Shape] {
        let channel = offsets.mask(for: "veil")
        return try #require(channel.source).picture(added: channel.added).shapes
    }

    private func settle() async {
        for _ in 0 ..< 6 {
            host.view.layoutIfNeeded()
            try? await Task.sleep(for: .milliseconds(30))
        }
    }

    private static func find<View: UIView>(_ type: View.Type, in view: UIView) -> View? {
        if let match = view as? View { return match }
        for child in view.subviews {
            if let match = find(type, in: child) { return match }
        }
        return nil
    }
}
