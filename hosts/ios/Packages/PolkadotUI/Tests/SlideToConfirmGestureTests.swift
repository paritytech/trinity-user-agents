import SwiftUI
import Testing
import UIKit
@testable import PolkadotUI

/// The slide sits inside a chat bubble whose swipe to reply is a UIKit pan,
/// inside a message list that scrolls. These tests build that real hierarchy
/// and check the ranking UIKit is given: touches on the control reach the
/// slide's pan, and both the reply pan and the list's scroll pan wait for it
/// to fail. It fails only for a drag that starts mostly vertical.
@MainActor
struct SlideToConfirmGestureTests {
    private struct Card: View, Hashable {
        var body: some View {
            CustomMessageWidgetView(
                node: CustomMessageWidgetNode(
                    content: .slideToConfirm(.init(text: "Slide to register", confirmAction: "register")),
                    modifiers: .init(fillWidth: true)
                ),
                resolveImage: nil
            )
            .frame(width: 280)
        }
    }

    private struct ChatScene {
        let window: UIWindow
        let list: UIScrollView
        let bubble: ChatMessageContainerView
        let slide: SlideToConfirmDragView

        var replyPan: UIPanGestureRecognizer? {
            bubble.gestureRecognizers?.compactMap { $0 as? UIPanGestureRecognizer }.first
        }
    }

    private func makeChatScene() throws -> ChatScene {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 393, height: 852))
        let list = UIScrollView(frame: window.bounds)
        list.contentSize = CGSize(width: window.bounds.width, height: 2_000)
        window.addSubview(list)

        let configuration = ChatMessageContainerConfiguration(
            innerContent: SwiftUIContentConfiguration(view: Card()),
            side: .leading,
            bubbleColor: .darkGray,
            identifier: "slide-card"
        )
        let bubble = try #require(configuration.makeContentView() as? ChatMessageContainerView)
        bubble.frame = CGRect(x: 0, y: 100, width: window.bounds.width, height: 120)
        list.addSubview(bubble)
        window.isHidden = false
        window.layoutIfNeeded()
        bubble.layoutIfNeeded()

        let slide = try #require(bubble.firstSubview(of: SlideToConfirmDragView.self))
        return ChatScene(window: window, list: list, bubble: bubble, slide: slide)
    }

    @Test("The control is drawn inside the chat bubble with its own pan")
    func controlCarriesItsPan() throws {
        let scene = try makeChatScene()

        #expect(scene.slide.gestureRecognizers?.contains(scene.slide.panRecognizer) == true)
        #expect(scene.slide.isDescendant(of: scene.bubble))
        #expect(scene.slide.bounds.width > 0)
        #expect(scene.slide.isInteractive)
    }

    @Test("A touch on the control lands on the view that owns the slide's pan")
    func touchesReachTheSlide() throws {
        let scene = try makeChatScene()
        let centre = scene.slide.convert(
            CGPoint(x: scene.slide.bounds.midX, y: scene.slide.bounds.midY),
            to: scene.window
        )

        let hit = try #require(scene.window.hitTest(centre, with: nil))

        #expect(hit === scene.slide || hit.isDescendant(of: scene.slide))
    }

    @Test("Swipe to reply waits for the slide to fail")
    func replyWaitsForTheSlide() throws {
        let scene = try makeChatScene()
        let replyPan = try #require(scene.replyPan)
        let slidePan = scene.slide.panRecognizer

        let replyMustWait = scene.slide.gestureRecognizer(slidePan, shouldBeRequiredToFailBy: replyPan)

        #expect(replyMustWait)
    }

    @Test("The message list's scroll waits for the slide to fail")
    func scrollWaitsForTheSlide() throws {
        let scene = try makeChatScene()
        let slidePan = scene.slide.panRecognizer

        #expect(scene.slide.gestureRecognizer(slidePan, shouldBeRequiredToFailBy: scene.list.panGestureRecognizer))
    }

    @Test("Pans outside the bubble's ancestry, and non-pan gestures, are not held back")
    func unrelatedGesturesAreLeftAlone() throws {
        let scene = try makeChatScene()
        let sibling = UIView()
        scene.list.addSubview(sibling)
        let siblingPan = UIPanGestureRecognizer()
        sibling.addGestureRecognizer(siblingPan)
        let bubbleTap = UITapGestureRecognizer()
        scene.bubble.addGestureRecognizer(bubbleTap)

        #expect(!scene.slide.outranks(siblingPan))
        #expect(!scene.slide.outranks(bubbleTap))
        #expect(!scene.slide.outranks(scene.slide.panRecognizer))
    }

    @Test("The slide claims horizontal drags in either direction and lets vertical ones scroll")
    func claimsOnlyHorizontalDrags() {
        #expect(SlideToConfirmDragView.claimsDrag(translation: CGPoint(x: 12, y: 3), velocity: .zero))
        // Right to left is the reply direction: on the control it still belongs to the slide.
        #expect(SlideToConfirmDragView.claimsDrag(translation: CGPoint(x: -12, y: 3), velocity: .zero))
        #expect(!SlideToConfirmDragView.claimsDrag(translation: CGPoint(x: 3, y: 12), velocity: .zero))
        #expect(SlideToConfirmDragView.claimsDrag(translation: .zero, velocity: CGPoint(x: 400, y: 20)))
        #expect(!SlideToConfirmDragView.claimsDrag(translation: .zero, velocity: .zero))
    }

    @Test("An inert control declines every drag, so the chat keeps its gestures")
    func inertControlDeclines() throws {
        let scene = try makeChatScene()
        scene.slide.isInteractive = false

        #expect(!scene.slide.gestureRecognizerShouldBegin(scene.slide.panRecognizer))
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
