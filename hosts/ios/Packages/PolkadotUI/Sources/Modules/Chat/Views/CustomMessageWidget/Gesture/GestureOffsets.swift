import QuartzCore
import SwiftUI

/// The offsets a worker sets on its keyed gestures between trees (`RendererNode.SetOffset`).
///
/// An offset update moves one gesture's children straight away: it is not a tree, so nothing is decoded, mapped or
/// diffed, and only the gesture view reading that key's channel draws again. A new tree drops every offset, because
/// the tree always wins.
public final class GestureOffsets {
    private var channels: [String: GestureOffsetChannel] = [:]

    public init() {}

    @MainActor
    public func channel(for key: String) -> GestureOffsetChannel {
        if let channel = channels[key] { return channel }
        let channel = GestureOffsetChannel()
        channels[key] = channel
        return channel
    }

    /// A `SetOffset` item for `key`.
    @MainActor
    public func set(key: String, offset: CGPoint) {
        channel(for: key).receive(offset)
    }

    /// A new tree arrived: it places every gesture itself.
    @MainActor
    public func treeArrived() {
        for channel in channels.values {
            channel.receive(nil)
        }
    }
}

/// One keyed gesture's offset, observed by its view alone.
@Observable
public final class GestureOffsetChannel {
    /// Added to where the current tree places the gesture's children; nil until the worker sets one.
    public private(set) var offset: CGPoint?
    /// Called whenever the worker answers: an offset for this key, or a new tree.
    @ObservationIgnored var onAnswer: (() -> Void)?

    @MainActor
    func receive(_ offset: CGPoint?) {
        if self.offset != offset { self.offset = offset }
        onAnswer?()
    }
}

private struct GestureOffsetsKey: EnvironmentKey {
    static let defaultValue: GestureOffsets? = nil
}

extension EnvironmentValues {
    var gestureOffsets: GestureOffsets? {
        get { self[GestureOffsetsKey.self] }
        set { self[GestureOffsetsKey.self] = newValue }
    }
}

/// When a keyed pan sends its moves: at most one per display frame, and none while the last one is unanswered. A
/// newer move replaces one not yet sent, so no backlog builds up. A move unanswered for `answerTimeout` no longer
/// holds the next one back, so a worker that answers late or not at all still hears the pan.
struct MovePacing {
    static let answerTimeout: TimeInterval = 0.1

    private(set) var pending: Data?
    private(set) var sentAt: TimeInterval?
    private(set) var dropped = 0

    mutating func offer(_ payload: Data) {
        if pending != nil { dropped += 1 }
        pending = payload
    }

    mutating func answered() {
        sentAt = nil
    }

    /// The move to send on the display frame at `now`, if any.
    mutating func frame(at now: TimeInterval) -> Data? {
        guard let payload = pending else { return nil }
        if let sentAt, now - sentAt < Self.answerTimeout { return nil }
        pending = nil
        sentAt = now
        return payload
    }

    mutating func reset() {
        pending = nil
        sentAt = nil
        dropped = 0
    }
}

/// Runs `MovePacing` on the display's frames while a keyed pan is under way.
final class GestureMovePacer: NSObject {
    private(set) var pacing = MovePacing()
    private var link: CADisplayLink?
    private var send: ((Data) -> Void)?

    func start(send: @escaping (Data) -> Void) {
        stop()
        self.send = send
        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func offer(_ payload: Data) {
        pacing.offer(payload)
        link?.isPaused = false
    }

    func answered() {
        pacing.answered()
    }

    /// Drops any move not yet sent: the end of a pan carries the final position itself.
    func stop() {
        link?.invalidate()
        link = nil
        send = nil
        pacing.reset()
    }

    @objc private func tick(_ link: CADisplayLink) {
        if let payload = pacing.frame(at: link.timestamp) {
            send?(payload)
        } else if pacing.pending == nil {
            link.isPaused = true
        }
    }
}
