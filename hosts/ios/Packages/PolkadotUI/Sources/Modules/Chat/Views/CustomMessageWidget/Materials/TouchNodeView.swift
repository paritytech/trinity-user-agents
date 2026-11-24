import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// A `Touch` node: its children, under a clear view that hands the finger to the core and sends the
/// actions it fires. A node that takes no touch draws its children alone, exactly as a box draws them.
struct TouchNodeView<Children: View>: View {
    let node: WidgetTouchNode
    @ViewBuilder let children: () -> Children

    @Environment(\.widgetMaterials) private var materials
    @Environment(\.widgetActionHandler) private var actionHandler
    /// Inside a gesture that reports where the finger is, the gesture takes the finger and hands it on.
    @Environment(\.gestureFinger) private var finger
    @State private var size: CGSize = .zero
    @State private var follower = TouchFeedFollower()

    var body: some View {
        let area = touchArea
        children()
            .environment(\.widgetTouchArea, area)
            .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0 }
            .onGeometryChange(for: CGPoint.self) { $0.frame(in: .global).origin } action: { follower.origin = $0 }
            .onChange(of: area.map(ObjectIdentifier.init), initial: true) { follow(area) }
            .onDisappear { finger?.unfollow(follower) }
            .overlay {
                if let area, finger == nil {
                    TouchCaptureView(area: area, drag: node.drag) { actions in
                        actions.forEach { actionHandler?($0, nil) }
                    }
                }
            }
            .accessibilityActions {
                ForEach(area == nil ? [] : node.accessibilityActions, id: \.self) { action in
                    Button(action) { actionHandler?(action, nil) }
                }
            }
    }

    private var touchArea: WidgetTouchArea? {
        guard node.takesTouch, size.width > 0, size.height > 0 else { return nil }
        return materials?.touchArea(index: node.index, size: size)
    }

    /// Takes the gesture's finger samples, moved from the screen into this node's frame.
    private func follow(_ area: WidgetTouchArea?) {
        guard let feed = finger else { return }
        guard let area else {
            feed.unfollow(follower)
            return
        }
        let handler = actionHandler
        feed.follow(follower) { [weak area, follower] phase, point in
            let local = CGPoint(x: point.x - follower.origin.x, y: point.y - follower.origin.y)
            area?.sample(phase, at: local).forEach { handler?($0, nil) }
        }
    }
}

/// Where a `Touch` node sits on screen, read when a sample arrives.
private final class TouchFeedFollower {
    var origin: CGPoint = .zero
}

private struct TouchCaptureView: UIViewRepresentable {
    let area: WidgetTouchArea
    let drag: WidgetTouchNode.Drag
    let fire: ([String]) -> Void

    func makeUIView(context _: Context) -> UIView {
        let view = UIView()
        view.backgroundColor = .clear
        view.addGestureRecognizer(TouchRecognizer())
        return view
    }

    func updateUIView(_ view: UIView, context _: Context) {
        guard let recognizer = view.gestureRecognizers?.first as? TouchRecognizer else { return }
        recognizer.area = area
        recognizer.drag = drag
        recognizer.fire = fire
    }
}

/// Reports `down` as the finger lands. For `any` it begins at once, which stops the chat from scrolling.
/// For `horizontal` it begins once the finger moved 10 points mostly sideways, and fails with `cancel`
/// when it moved mostly up or down, so the chat scrolls.
private final class TouchRecognizer: UIGestureRecognizer {
    static let slop: CGFloat = 10

    var area: WidgetTouchArea?
    var drag: WidgetTouchNode.Drag = .any
    var fire: ([String]) -> Void = { _ in }
    private var tracked: UITouch?
    private var start: CGPoint = .zero

    init() {
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)
        guard tracked == nil, let touch = touches.first else { return }
        tracked = touch
        start = touch.location(in: view)
        report(.down, at: start)
        if drag == .any { state = .began }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesMoved(touches, with: event)
        guard let tracked, touches.contains(tracked) else { return }
        let point = tracked.location(in: view)
        if state == .possible, drag == .horizontal {
            let moved = CGPoint(x: point.x - start.x, y: point.y - start.y)
            if hypot(moved.x, moved.y) >= Self.slop {
                guard abs(moved.x) > abs(moved.y) else {
                    report(.cancel, at: point)
                    state = .failed
                    return
                }
                state = .began
            }
        }
        report(.move, at: point)
        if state == .began || state == .changed { state = .changed }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesEnded(touches, with: event)
        guard let tracked, touches.contains(tracked) else { return }
        report(.up, at: tracked.location(in: view))
        state = .ended
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesCancelled(touches, with: event)
        guard let tracked, touches.contains(tracked) else { return }
        report(.cancel, at: tracked.location(in: view))
        state = state == .possible ? .failed : .cancelled
    }

    override func reset() {
        super.reset()
        tracked = nil
    }

    /// The chat scrolls only once this recognizer failed, so a drag it keeps never moves the list.
    override func shouldBeRequiredToFail(by other: UIGestureRecognizer) -> Bool {
        other.view is UIScrollView
    }

    private func report(_ phase: WidgetTouchPhase, at point: CGPoint) {
        guard let area else { return }
        let actions = area.sample(phase, at: point)
        if !actions.isEmpty { fire(actions) }
    }
}
