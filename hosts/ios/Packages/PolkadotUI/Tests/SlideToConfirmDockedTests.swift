import SwiftUI
import Testing
import UIKit
@testable import PolkadotUI

/// A product can dock a slide to confirm in the footer of its room, where every docked card stands
/// 80 tall. These tests cover what the footer needs from the control: it takes the height the product
/// gives it, and a slide on the docked card reaches the card's action handler, the same path a press
/// on any of its buttons takes.
@MainActor
struct SlideToConfirmDockedTests {
    @MainActor
    @Observable
    final class Tree: WidgetNodeProviding {
        var node: CustomMessageWidgetNode?

        init(_ node: CustomMessageWidgetNode?) {
            self.node = node
        }
    }

    private static let dockedModifiers = CustomMessageWidgetNode.Modifiers(height: 80, fillWidth: true)
    private static let width: CGFloat = 342

    private static func slider(
        _ modifiers: CustomMessageWidgetNode.Modifiers? = nil,
        action: String? = "register"
    ) -> CustomMessageWidgetNode {
        CustomMessageWidgetNode(
            content: .slideToConfirm(.init(
                text: "Slide to register",
                secondaryText: "Today at 20:00",
                confirmAction: action
            )),
            modifiers: modifiers ?? dockedModifiers
        )
    }

    // MARK: Height

    @Test("A control given no height keeps the track's own 72, with the thumb 8 from its edges")
    func ownHeight() {
        let height = SlideToConfirmMetrics.thumbTrackHeight(given: .init(fillWidth: true))

        #expect(height == 72)
        #expect(SlideToConfirmMetrics.thumbInset(trackHeight: height) == 8)
    }

    @Test("A control given the room footer's 80 stands 80, with the thumb 12 from its edges like the web pill")
    func footerHeight() {
        let height = SlideToConfirmMetrics.thumbTrackHeight(given: Self.dockedModifiers)

        #expect(height == 80)
        #expect(SlideToConfirmMetrics.thumbInset(trackHeight: height) == 12)
    }

    @Test("A height too short for the thumb still holds the thumb")
    func tooShort() {
        let height = SlideToConfirmMetrics.thumbTrackHeight(given: .init(height: 40))

        #expect(height == 56)
        #expect(SlideToConfirmMetrics.thumbInset(trackHeight: height) == 0)
    }

    /// The track used to stay 72 inside an 80 frame, leaving a 4pt strip of nothing above and below
    /// it. Drawn, the track now reaches the frame's top and bottom edges.
    @Test("Drawn at the footer's 80, the track fills the frame from edge to edge")
    func drawnTrackFillsTheFrame() throws {
        let renderer = ImageRenderer(
            content: NodeSlideToConfirmView(
                props: .init(text: "Slide to register", confirmAction: "register"),
                modifiers: Self.dockedModifiers
            )
            .frame(width: Self.width)
        )
        renderer.scale = 1
        let image = try #require(renderer.cgImage)

        #expect(image.height == 80)
        #expect(try alpha(of: image, x: image.width / 2, y: 1) > 0.5)
        #expect(try alpha(of: image, x: image.width / 2, y: image.height - 2) > 0.5)
    }

    // MARK: The press path

    private func dockedCard(
        tree: Tree,
        onAction: @escaping WidgetActionHandler
    ) async throws -> (UIWindow, SlideToConfirmDragView) {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
        let host = UIHostingController(
            rootView: ProductWidgetChatView(
                messageId: "room",
                nodeProvider: tree,
                onAction: onAction,
                resolveImage: WidgetImageResolver { _ in nil }
            )
            .frame(width: Self.width)
        )
        host.view.frame = CGRect(x: 24, y: 700, width: Self.width, height: 80)
        window.rootViewController = host
        window.isHidden = false
        window.layoutIfNeeded()
        host.view.layoutIfNeeded()
        // The control learns its width from a geometry change, which lands after the first layout pass.
        try await Task.sleep(for: .milliseconds(200))
        let surface = try #require(host.view.firstSubview(of: SlideToConfirmDragView.self))
        return (window, surface)
    }

    @Test("A full slide on the docked card sends its confirm action once, through the card's action handler")
    func fullSlideSendsTheAction() async throws {
        var actions: [String] = []
        let (window, surface) = try await dockedCard(tree: Tree(Self.slider())) { actionId, _ in
            actions.append(actionId)
        }

        surface.onChange?(Self.width)
        surface.onEnd?(true)
        surface.onChange?(Self.width)
        surface.onEnd?(true)

        #expect(actions == ["register"])
        window.isHidden = true
    }

    @Test("A slide that stops short, or that the system cancels, sends nothing")
    func shortOrCancelledSlideSendsNothing() async throws {
        var actions: [String] = []
        let (window, surface) = try await dockedCard(tree: Tree(Self.slider())) { actionId, _ in
            actions.append(actionId)
        }

        surface.onChange?(Self.width / 3)
        surface.onEnd?(true)
        surface.onChange?(Self.width)
        surface.onEnd?(false)

        #expect(actions.isEmpty)
        window.isHidden = true
    }

    private func alpha(of image: CGImage, x: Int, y: Int) throws -> CGFloat {
        var pixel: [UInt8] = [0, 0, 0, 0]
        let context = try #require(CGContext(
            data: &pixel,
            width: 1,
            height: 1,
            bitsPerComponent: 8,
            bytesPerRow: 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ))
        context.draw(image, in: CGRect(x: -x, y: y - image.height + 1, width: image.width, height: image.height))
        return CGFloat(pixel[3]) / 255
    }
}

private extension UIView {
    func firstSubview<View: UIView>(of type: View.Type) -> View? {
        for subview in subviews {
            if let match = subview as? View ?? subview.firstSubview(of: type) {
                return match
            }
        }
        return nil
    }
}
