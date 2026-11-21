import QuartzCore
import SwiftUI

/// Reports a pan's event on the pan's action id.
public typealias WidgetPanHandler = (String, WidgetPanEvent) -> Void

private struct WidgetMotionKey: EnvironmentKey {
    static let defaultValue: WidgetMotion? = nil
}

private struct WidgetPansEnabledKey: EnvironmentKey {
    static let defaultValue = false
}

private struct WidgetPerspectiveKey: EnvironmentKey {
    static let defaultValue: CGFloat? = nil
}

extension EnvironmentValues {
    var widgetMotion: WidgetMotion? {
        get { self[WidgetMotionKey.self] }
        set { self[WidgetMotionKey.self] = newValue }
    }

    var widgetPansEnabled: Bool {
        get { self[WidgetPansEnabledKey.self] }
        set { self[WidgetPansEnabledKey.self] = newValue }
    }

    /// The perspective of the nearest ancestor that declared one.
    var widgetPerspective: CGFloat? {
        get { self[WidgetPerspectiveKey.self] }
        set { self[WidgetPerspectiveKey.self] = newValue }
    }
}

public extension View {
    /// Whether pans in the product-drawn bodies below follow a finger. Off by default: a surface whose own drags
    /// would compete, such as a scrolling list, draws each pan at the value the product declared.
    func widgetPansEnabled(_ enabled: Bool) -> some View {
        environment(\.widgetPansEnabled, enabled)
    }
}

extension CustomMessageWidgetNode {
    /// Every pan in the tree, depth first.
    var pans: [Pan] {
        let own = modifiers.pan.map { [$0] } ?? []
        return own + children.flatMap(\.pans)
    }

    private var children: [CustomMessageWidgetNode] {
        switch content {
        case let .box(_, children),
             let .column(_, children),
             let .row(_, children):
            children
        case .spacer,
             .text,
             .button,
             .textField,
             .image:
            []
        }
    }
}

/// A node's drives and pan: its transform and opacity each frame, and the drag and accessibility action that move
/// its pan.
struct NodeMotionModifier: ViewModifier {
    let modifiers: CustomMessageWidgetNode.Modifiers

    @Environment(\.widgetMotion) private var motion
    @Environment(\.widgetPerspective) private var perspective

    func body(content: Content) -> some View {
        let driven = DrivenValues(drives: modifiers.drives, motion: motion)
        content
            .environment(\.widgetPerspective, modifiers.perspective ?? perspective)
            .modifier(DrivenTransformEffect(values: driven, perspective: perspective))
            .opacity(driven.opacity)
            // A face turned away is hidden by its opacity, and must not take the press meant for the one in front.
            .allowsHitTesting(driven.opacity > 0)
            .modifier(NodePanModifier(pan: modifiers.pan))
    }
}

/// The driven properties of one node at the pans' current values.
private struct DrivenValues {
    var rotation = SIMD3<Double>()
    var scale = 1.0
    var opacity = 1.0
    var translation = CGSize.zero

    @MainActor
    init(drives: [CustomMessageWidgetNode.Drive], motion: WidgetMotion?) {
        for drive in drives {
            guard let reading = motion?.reading(drive.source) else { continue }

            let value = drive.value(at: reading.value, period: reading.period)
            switch drive.property {
            case .rotationX: rotation.x = value
            case .rotationY: rotation.y = value
            case .rotationZ: rotation.z = value
            case .scale: scale = value
            case .opacity: opacity = value
            case .translationX: translation.width = value
            case .translationY: translation.height = value
            }
        }
    }
}

/// Rotates and scales about the node's centre, seen from `perspective` points away, then offsets it.
private struct DrivenTransformEffect: GeometryEffect {
    let values: DrivenValues
    let perspective: CGFloat?

    func effectValue(size: CGSize) -> ProjectionTransform {
        let centre = CATransform3DMakeTranslation(-size.width / 2, -size.height / 2, 0)
        var transform = CATransform3DConcat(centre, rotation)
        transform = CATransform3DConcat(transform, CATransform3DMakeScale(values.scale, values.scale, 1))
        if let perspective, perspective > 0 {
            var projection = CATransform3DIdentity
            projection.m34 = -1 / perspective
            transform = CATransform3DConcat(transform, projection)
        }
        let back = CATransform3DMakeTranslation(
            size.width / 2 + values.translation.width,
            size.height / 2 + values.translation.height,
            0
        )
        return ProjectionTransform(CATransform3DConcat(transform, back))
    }

    private var rotation: CATransform3D {
        let radians = values.rotation * .pi / 180
        let aboutX = CATransform3DMakeRotation(radians.x, 1, 0, 0)
        let aboutY = CATransform3DMakeRotation(radians.y, 0, 1, 0)
        let aboutZ = CATransform3DMakeRotation(radians.z, 0, 0, 1)
        return CATransform3DConcat(CATransform3DConcat(aboutX, aboutY), aboutZ)
    }
}

/// The drag that moves a pan on a surface that lets pans follow a finger, and its accessibility action anywhere.
private struct NodePanModifier: ViewModifier {
    let pan: CustomMessageWidgetNode.Pan?

    @Environment(\.widgetMotion) private var motion
    @Environment(\.widgetPansEnabled) private var pansEnabled
    @State private var extent = CGSize.zero
    /// Whether the press under way is this pan's, decided on its first move. Nil before then.
    @State private var claimed: Bool?
    @GestureState private var isPressed = false

    func body(content: Content) -> some View {
        if let pan, let motion {
            content
                .onGeometryChange(for: CGSize.self, of: \.size) { extent = $0 }
                .gesture(drag(pan, motion), including: pansEnabled ? .all : .subviews)
                .onChange(of: isPressed) { _, pressed in
                    guard !pressed else { return }

                    // A press the system takes ends without `onEnded`. One that ended has run it by the next turn,
                    // and cleared the claim.
                    Task { @MainActor in
                        guard claimed == true else { return }

                        claimed = nil
                        motion.cancelDrag(pan.id)
                    }
                }
                .modifier(PanAccessibilityAction(label: pan.label) { motion.step(pan.id) })
        } else {
            content
        }
    }

    private func drag(_ pan: CustomMessageWidgetNode.Pan, _ motion: WidgetMotion) -> some Gesture {
        DragGesture(minimumDistance: 10)
            .updating($isPressed) { _, pressed, _ in pressed = true }
            .onChanged { value in
                if claimed == nil {
                    claimed = isAlong(pan.axis, value.translation)
                    if claimed == true { motion.beginDrag(pan.id) }
                }
                guard claimed == true else { return }

                motion.drag(pan.id, travel: along(pan.axis, value.translation), extent: along(pan.axis, extent))
            }
            .onEnded { value in
                defer { claimed = nil }
                guard claimed == true else { return }

                motion.release(
                    pan.id,
                    pointsPerSecond: along(pan.axis, value.velocity),
                    extent: along(pan.axis, extent)
                )
            }
    }

    private func isAlong(_ axis: CustomMessageWidgetNode.Pan.Axis, _ translation: CGSize) -> Bool {
        switch axis {
        case .horizontal: abs(translation.width) > abs(translation.height)
        case .vertical: abs(translation.height) > abs(translation.width)
        }
    }

    private func along(_ axis: CustomMessageWidgetNode.Pan.Axis, _ size: CGSize) -> Double {
        switch axis {
        case .horizontal: size.width
        case .vertical: size.height
        }
    }
}

/// The screen reader's way to move a pan: one step forward, named by the product.
private struct PanAccessibilityAction: ViewModifier {
    let label: String?
    let step: () -> Void

    func body(content: Content) -> some View {
        if let label {
            content.accessibilityAction(named: Text(verbatim: label), step)
        } else {
            content
        }
    }
}
