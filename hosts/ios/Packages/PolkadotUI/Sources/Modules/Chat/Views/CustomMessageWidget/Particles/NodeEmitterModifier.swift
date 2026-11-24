import QuartzCore
import SwiftUI
import UIKit

/// Counts the actions the body's current tree has sent, by name, so emitters can burst on them with no round trip to
/// the worker.
@Observable
public final class WidgetActionEvents {
    public private(set) var sent: [String: Int] = [:]

    public init() {}

    /// Called where actions are sent, on the main thread.
    func record(_ action: String) {
        sent[action, default: 0] += 1
    }
}

private struct WidgetActionEventsKey: EnvironmentKey {
    static let defaultValue: WidgetActionEvents? = nil
}

extension EnvironmentValues {
    var widgetActionEvents: WidgetActionEvents? {
        get { self[WidgetActionEventsKey.self] }
        set { self[WidgetActionEventsKey.self] = newValue }
    }
}

/// Draws a node's particles over its content and children, in its frame and unclipped, taking no room in the layout.
/// Live particles finish after a tree drops the emitter. None are sent where the person asked for less motion.
struct NodeEmitterModifier: ViewModifier {
    let emitter: CustomMessageWidgetNode.Emitter?

    /// The emitter a tree dropped, kept while its particles finish.
    @State private var finishing: CustomMessageWidgetNode.Emitter?

    func body(content: Content) -> some View {
        content
            .overlay {
                if let drawn = emitter ?? finishing {
                    ParticleLayer(emitter: drawn, live: emitter != nil) { finishing = nil }
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
            }
            .onChange(of: emitter) { before, after in
                if after == nil, before != nil { finishing = before }
                if after != nil { finishing = nil }
            }
    }
}

/// The SwiftUI side of a node's particles: what sets them off, where the pan's finger is, and the looks' pictures.
private struct ParticleLayer: View {
    let emitter: CustomMessageWidgetNode.Emitter
    /// False once the tree dropped the emitter: nothing more is sent, and `finished` runs when the last one dies.
    let live: Bool
    let finished: () -> Void

    @Environment(\.gestureOffsets) private var offsets
    @Environment(\.widgetActionEvents) private var actionEvents
    @Environment(\.widgetImageResolver) private var resolveImage
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.layoutDirection) private var layoutDirection
    @State private var geometry = MaskGeometry()
    @State private var images: [CustomMessageWidgetNode.ImageSource: UIImage] = [:]
    /// Particles asked for by bursts so far; the surface sends the ones it has not yet sent.
    @State private var burstTotal = 0

    var body: some View {
        let sending = live && !reduceMotion
        ParticleSurface(
            emitter: emitter,
            sending: sending,
            fingerDown: sending && touch?.isDown == true,
            finger: finger,
            mirrored: layoutDirection == .rightToLeft,
            images: images,
            burstTotal: burstTotal,
            finished: finished
        )
        .onGeometryChange(for: MaskGeometry.self) { MaskGeometry($0) } action: { geometry = $0 }
        .onAppear { burst(on: .appear) }
        .onChange(of: actionCounts) { before, after in
            for (name, count) in after where count > before[name] ?? 0 {
                burst(on: .action(name))
            }
        }
        .onChange(of: changeValues) { before, after in
            for (index, value) in after.enumerated() where index < before.count && before[index] != value {
                burst(on: .change(value))
            }
        }
        .task(id: imageSources) { await loadImages() }
    }
}

private extension ParticleLayer {
    var touch: PanTouch? {
        guard case let .pan(key) = emitter.source else { return nil }
        return offsets?.touch(for: key)
    }

    var actionCounts: [String: Int] {
        var counts: [String: Int] = [:]
        for case let .action(name) in emitter.bursts.map(\.on) {
            counts[name] = actionEvents?.sent[name] ?? 0
        }
        return counts
    }

    var changeValues: [UInt32] {
        emitter.bursts.compactMap { if case let .change(value) = $0.on { value } else { nil } }
    }

    var imageSources: [CustomMessageWidgetNode.ImageSource] {
        emitter.looks.compactMap { if case let .image(source?, _) = $0 { source } else { nil } }
    }

    /// The pan source's finger in the node's frame from its top-start corner, read from the pan's trail as a followed
    /// mask reads it.
    var finger: () -> CGPoint? {
        guard case let .pan(key) = emitter.source, let offsets else { return { nil } }
        let trail = offsets.trail(for: key)
        let geometry = geometry
        let mirrored = layoutDirection == .rightToLeft
        return {
            guard let last = trail.strokes.last?.last else { return nil }
            let scale = geometry.scale
            let x = (last.x + trail.origin.x - geometry.frame.minX) / scale.width
            let y = (last.y + trail.origin.y - geometry.frame.minY) / scale.height
            return CGPoint(x: mirrored ? geometry.size.width - x : x, y: y)
        }
    }

    func burst(on trigger: CustomMessageWidgetNode.Emitter.Trigger) {
        guard live, !reduceMotion else { return }
        burstTotal += emitter.bursts.filter { $0.on == trigger }.reduce(0) { $0 + $1.count }
    }

    func loadImages() async {
        for source in imageSources where images[source] == nil {
            if let image = await resolveImage?(source) { images[source] = image }
        }
    }
}

/// Draws particles as layers over the node, outside its bounds too, moved on each display frame while any is alive or
/// the source sends.
private struct ParticleSurface: UIViewRepresentable {
    let emitter: CustomMessageWidgetNode.Emitter
    let sending: Bool
    let fingerDown: Bool
    let finger: () -> CGPoint?
    let mirrored: Bool
    let images: [CustomMessageWidgetNode.ImageSource: UIImage]
    let burstTotal: Int
    let finished: () -> Void

    func makeUIView(context _: Context) -> ParticleSurfaceView {
        ParticleSurfaceView()
    }

    func updateUIView(_ view: ParticleSurfaceView, context _: Context) {
        view.emitter = emitter
        view.sending = sending
        view.fingerDown = fingerDown
        view.finger = finger
        view.mirrored = mirrored
        view.images = images
        view.finished = finished
        view.burst(upTo: burstTotal)
        view.run()
    }

    static func dismantleUIView(_ view: ParticleSurfaceView, coordinator _: ()) {
        view.stop()
    }
}

final class ParticleSurfaceView: UIView {
    var emitter: CustomMessageWidgetNode.Emitter?
    var sending = false
    var fingerDown = false
    var finger: () -> CGPoint? = { nil }
    var mirrored = false
    var images: [CustomMessageWidgetNode.ImageSource: UIImage] = [:]
    var finished: (() -> Void)?

    private var field = ParticleField()
    private var link: CADisplayLink?
    private var burstsSent: Int?
    private var drawn: [CALayer] = []

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        clipsToBounds = false
        backgroundColor = .clear
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) { fatalError() }

    /// Sends the particles bursts asked for since the last call; the first call only notes where the count starts.
    func burst(upTo total: Int) {
        defer { burstsSent = total }
        guard let sent = burstsSent, total > sent, let emitter else { return }
        field.burst(emitter, count: total - sent, at: CACurrentMediaTime(), finger: finger(), size: bounds.size)
    }

    /// Runs the display link while anything moves or may be sent.
    func run() {
        guard needsFrames else { return }
        if link == nil {
            let link = CADisplayLink(target: self, selector: #selector(tick))
            link.add(to: .main, forMode: .common)
            self.link = link
            field.resume()
        } else if link?.isPaused == true {
            field.resume()
        }
        link?.isPaused = false
    }

    func stop() {
        link?.invalidate()
        link = nil
    }

    private var sends: Bool {
        guard sending, let emitter, !emitter.looks.isEmpty else { return false }
        switch emitter.source {
        case .pan: return fingerDown
        case .point,
             .area: return emitter.ratePerSecond > 0
        }
    }

    private var needsFrames: Bool { sends || !field.isEmpty }

    @objc private func tick() {
        let now = CACurrentMediaTime()
        field.tick(emitter, sending: sends, at: now, finger: fingerDown ? finger() : nil, size: bounds.size)
        draw(at: now)
        if !needsFrames {
            link?.isPaused = true
            if !sending { finished?() }
        }
    }

    private func draw(at now: TimeInterval) {
        guard let emitter else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        while drawn.count < field.particles.count {
            let sublayer = CALayer()
            layer.addSublayer(sublayer)
            drawn.append(sublayer)
        }
        for (index, sublayer) in drawn.enumerated() {
            guard index < field.particles.count,
                  let state = field.drawn(field.particles[index], emitter, at: now),
                  field.particles[index].look < emitter.looks.count
            else {
                sublayer.isHidden = true
                continue
            }
            style(sublayer, emitter.looks[field.particles[index].look])
            let point = state.place.point
            sublayer.isHidden = false
            sublayer.position = CGPoint(x: mirrored ? bounds.width - point.x : point.x, y: point.y)
            sublayer.opacity = Float(state.place.opacity)
            sublayer.setAffineTransform(
                CGAffineTransform(rotationAngle: state.turn * .pi / 180).scaledBy(
                    x: state.place.scale,
                    y: state.place.scale
                )
            )
        }
        CATransaction.commit()
    }

    private func style(_ sublayer: CALayer, _ look: CustomMessageWidgetNode.Emitter.Look) {
        sublayer.bounds = CGRect(origin: .zero, size: look.size)
        switch look {
        case let .circle(diameter, color):
            sublayer.contents = nil
            sublayer.backgroundColor = UIColor(color).cgColor
            sublayer.cornerRadius = diameter / 2
        case let .rectangle(_, color):
            sublayer.contents = nil
            sublayer.backgroundColor = UIColor(color).cgColor
            sublayer.cornerRadius = 0
        case let .image(source, _):
            sublayer.backgroundColor = nil
            sublayer.cornerRadius = 0
            sublayer.contents = source.flatMap { images[$0] }?.cgImage
            sublayer.contentsGravity = .resize
        }
    }
}
