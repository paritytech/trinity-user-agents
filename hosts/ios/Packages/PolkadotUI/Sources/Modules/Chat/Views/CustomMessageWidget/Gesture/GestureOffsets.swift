import QuartzCore
import UIKit
import SwiftUI

/// The offsets a worker sets on its keyed gestures between trees (`RendererNode.SetOffset`).
///
/// An offset update moves one gesture's children straight away: it is not a tree, so nothing is decoded, mapped or
/// diffed, and only the gesture view reading that key's channel draws again. A new tree drops every offset, because
/// the tree always wins.
///
/// An offset is first handed over from the render stream's thread (`deliver`) and then applied on the main thread,
/// either by the frame that sent the move, which waits a moment for it (`GestureMovePacer`), or by the stream's own hop
/// to the main thread (`applyDelivered`), whichever comes first. Each answer is applied once.
public final class GestureOffsets {
    private var channels: [String: GestureOffsetChannel] = [:]
    private var places: [String: GesturePanPlace] = [:]
    let answers = GestureAnswerMailbox()

    public init() {}

    @MainActor
    public func channel(for key: String) -> GestureOffsetChannel {
        if let channel = channels[key] { return channel }
        let channel = GestureOffsetChannel()
        channels[key] = channel
        return channel
    }

    /// Where the keyed gesture's children stand, for the nodes that fade with it.
    @MainActor
    public func place(for key: String) -> GesturePanPlace {
        if let place = places[key] { return place }
        let place = GesturePanPlace()
        places[key] = place
        return place
    }

    /// A `SetOffset` item for `key`.
    @MainActor
    public func set(key: String, offset: CGPoint) {
        channel(for: key).receive(offset)
    }

    /// A `SetOffset` item for `key`, from any thread: kept until the main thread applies it.
    public func deliver(key: String, offset: CGPoint) {
        answers.deliver(GestureAnswer(key: key, offset: offset))
    }

    /// Applies the newest offset delivered and not yet applied, if any.
    @MainActor
    public func applyDelivered() {
        guard let answer = answers.take() else { return }
        set(key: answer.key, offset: answer.offset)
    }

    /// A new tree arrived: it places every gesture itself.
    @MainActor
    public func treeArrived() {
        _ = answers.take()
        for channel in channels.values {
            channel.receive(nil)
        }
    }
}

struct GestureAnswer: Equatable {
    var key: String
    var offset: CGPoint
}

/// Hands a worker's offset answers from the render stream's thread to the main thread. Only the newest one not yet
/// applied is kept, and a waiting frame wakes as soon as one arrives.
final class GestureAnswerMailbox: @unchecked Sendable {
    private let condition = NSCondition()
    private var newest: GestureAnswer?
    private var count = 0

    /// How many answers have been delivered so far.
    var delivered: Int {
        condition.lock()
        defer { condition.unlock() }
        return count
    }

    func deliver(_ answer: GestureAnswer) {
        condition.lock()
        newest = answer
        count += 1
        condition.broadcast()
        condition.unlock()
    }

    /// The newest answer not yet applied, which is then applied by the caller alone.
    func take() -> GestureAnswer? {
        condition.lock()
        defer { condition.unlock() }
        let answer = newest
        newest = nil
        return answer
    }

    /// Waits until an answer arrives after the first `after` ones, or `deadline` passes; true when one did.
    func wait(after: Int, until deadline: Date) -> Bool {
        condition.lock()
        defer { condition.unlock() }
        while count <= after {
            if !condition.wait(until: deadline) { return count > after }
        }
        return true
    }
}

/// One keyed gesture's offset, observed by its view alone.
@Observable
public final class GestureOffsetChannel {
    /// Added to where the current tree places the gesture's children; nil until the worker sets one.
    public private(set) var offset: CGPoint?
    /// Called whenever the worker answers: an offset for this key, or a new tree.
    @ObservationIgnored var onAnswer: (() -> Void)?
    /// When the last release settle began (media time); a pan starting before it is over cuts it short.
    @ObservationIgnored private(set) var settleStarted: TimeInterval?
    /// From a release until the worker's next tree, a late answer to a move must not pull the settling thumb back.
    @ObservationIgnored private(set) var holdsSettle = false

    @MainActor
    func receive(_ offset: CGPoint?) {
        if offset == nil { holdsSettle = false }
        if offset == nil || !holdsSettle, self.offset != offset { self.offset = offset }
        onAnswer?()
    }

    /// Moves the thumb to `target` with `animation` (nil: at once): the release settle, drawn by the host alone.
    @MainActor
    func settle(to target: CGPoint, animation: Animation?, now: TimeInterval) {
        settleStarted = now
        holdsSettle = true
        withAnimation(animation) { offset = target }
    }

    /// A settle that may still be moving the thumb at `now`.
    func isSettling(at now: TimeInterval) -> Bool {
        guard let settleStarted else { return false }
        return now - settleStarted < GestureSettle.longest
    }

    /// A new pan began: tracking takes over at once.
    @MainActor
    func endSettle() {
        settleStarted = nil
        holdsSettle = false
    }
}

/// How a keyed every-move thumb settles when the finger lifts, on the host, so it does not jump in one frame: back home
/// with the native slide-to-confirm's spring, or, released at or past the worker's completion point (90% of the
/// travel, as its slider decides), to the end with a short ease-out. Tracking while the finger is down stays direct.
enum GestureSettle: Equatable {
    case home
    case end

    static let completionShare: CGFloat = 0.9
    static let endDuration: TimeInterval = 0.14
    /// About how long the spring takes to come to rest (it overshoots once); a pan in this time cuts it short.
    static let longest: TimeInterval = 0.7

    static func kind(released along: CGFloat, travel: CGFloat, cancelled: Bool) -> GestureSettle {
        !cancelled && travel > 0 && along >= travel * completionShare ? .end : .home
    }

    /// The channel offset that puts the thumb there. Offsets count from where the worker's last tree drew the thumb,
    /// `drawnAt`, which is the place of the move it last answered minus that answer, in whole points as it draws them.
    static func target(
        _ kind: GestureSettle,
        axis: CustomMessageWidgetNode.PanAxis,
        travel: CGFloat,
        drawnAt: CGPoint
    ) -> CGPoint {
        let place = kind == .end ? GestureMotion.end(axis: axis, travel: travel) : .zero
        return CGPoint(x: place.x - drawnAt.x, y: place.y - drawnAt.y)
    }

    static func drawnAt(answeredMove: CGPoint, answer: CGPoint) -> CGPoint {
        CGPoint(x: (answeredMove.x - answer.x).rounded(), y: (answeredMove.y - answer.y).rounded())
    }

    static func animation(_ kind: GestureSettle, reduceMotion: Bool) -> Animation? {
        if reduceMotion { return nil }
        switch kind {
        case .home: return .spring(response: 0.35, dampingFraction: 0.6)
        case .end: return .easeOut(duration: endDuration)
        }
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

/// How a keyed pan's moves meet the display's frames. Read once at launch from the app's settings, so one build can
/// run each way:
/// - `GestureSendOnTouch` (default on): a move goes out while the touch is handled, at the start of the frame, rather
///   than at the frame's display-link callback; `NO` keeps the callback.
/// - `GestureAnswerWaitMs` (default 4): after sending a move, the frame waits up to this long for the worker's answer,
/// so
///   the answer is drawn in the same frame as the touch; `0` applies answers whenever they reach the main thread.
/// (`simctl spawn <device> defaults write io.parity.polkadotapp.develop GestureAnswerWaitMs -int 0`)
enum GesturePacingSettings {
    static let defaultWait: TimeInterval = 0.004
    static let longestWait: TimeInterval = 0.010

    static func sendOnTouch(setting: Any?) -> Bool {
        (setting as? Bool) ?? (setting as? NSNumber)?.boolValue ?? true
    }

    /// Seconds; out-of-range values are clamped to what one 60 Hz frame can spare.
    static func answerWait(setting: Any?) -> TimeInterval {
        guard let milliseconds = (setting as? NSNumber)?.doubleValue ?? (setting as? String).flatMap(Double.init) else {
            return defaultWait
        }
        return min(max(milliseconds / 1_000, 0), longestWait)
    }

    static let currentSendOnTouch = sendOnTouch(setting: UserDefaults.standard.object(forKey: "GestureSendOnTouch"))
    static let currentAnswerWait = answerWait(setting: UserDefaults.standard.object(forKey: "GestureAnswerWaitMs"))
}

/// When a keyed pan sends its moves: at most one per display refresh, and none while the last one is unanswered. A
/// newer move replaces one not yet sent, so no backlog builds up. A move unanswered for `answerTimeout` no longer
/// holds the next one back, so a worker that answers late or not at all still hears the pan.
///
/// A move can go out as soon as it is offered (`touch`), or on the display link's callback (`frame`). "One per
/// refresh" is counted from the last send's time, not from the callbacks: a display link made for a new pan first
/// fires up to two refreshes late, and a rule tied to it held the pan's second move back for a whole refresh.
struct MovePacing {
    static let answerTimeout: TimeInterval = 0.1

    /// Sends closer together than this are in the same refresh: 60% of a refresh, so a touch delivered a little early
    /// or late still counts as the next one.
    let minimumGap: TimeInterval
    private(set) var pending: Data?
    private(set) var sentAt: TimeInterval?
    private(set) var lastSend: TimeInterval?
    private(set) var dropped = 0

    init(refresh: TimeInterval = 1.0 / 60) {
        minimumGap = refresh * 0.6
    }

    mutating func offer(_ payload: Data) {
        if pending != nil { dropped += 1 }
        pending = payload
    }

    mutating func answered() {
        sentAt = nil
    }

    private func free(at now: TimeInterval) -> Bool {
        if let lastSend, now - lastSend < minimumGap { return false }
        guard let sentAt else { return true }
        return now - sentAt >= Self.answerTimeout
    }

    private mutating func take(at now: TimeInterval) -> Data? {
        guard let payload = pending, free(at: now) else { return nil }
        pending = nil
        sentAt = now
        lastSend = now
        return payload
    }

    /// The move to send at once, while its touch is handled, if nothing holds it back.
    mutating func touch(at now: TimeInterval) -> Data? {
        take(at: now)
    }

    /// The move to send on the display frame at `now`, if any.
    mutating func frame(at now: TimeInterval) -> Data? {
        take(at: now)
    }

    mutating func reset() {
        pending = nil
        sentAt = nil
        lastSend = nil
        dropped = 0
    }
}

/// Runs `MovePacing` on the display's frames while a keyed pan is under way.
final class GestureMovePacer: NSObject {
    private(set) var pacing = MovePacing(refresh: 1.0 / Double(max(UIScreen.main.maximumFramesPerSecond, 1)))
    private var link: CADisplayLink?
    private var send: ((Data) -> Void)?
    private var offsets: GestureOffsets?
    private var sendOnTouch = GesturePacingSettings.currentSendOnTouch
    private var answerWait = GesturePacingSettings.currentAnswerWait

    func start(
        send: @escaping (Data) -> Void,
        offsets: GestureOffsets?,
        sendOnTouch: Bool = GesturePacingSettings.currentSendOnTouch,
        answerWait: TimeInterval = GesturePacingSettings.currentAnswerWait
    ) {
        stop()
        self.send = send
        self.offsets = offsets
        self.sendOnTouch = sendOnTouch
        self.answerWait = answerWait
        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    /// The place of the move last sent, which the worker's next answer is for (one move is in flight at a time).
    private(set) var sentPlace: CGPoint?
    private var pendingPlace: CGPoint?

    func offer(_ payload: Data, place: CGPoint? = nil) {
        pendingPlace = place
        pacing.offer(payload)
        if sendOnTouch, link != nil, let payload = pacing.touch(at: CACurrentMediaTime()) {
            deliver(payload)
        }
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
        offsets = nil
        sentPlace = nil
        pendingPlace = nil
        pacing.reset()
    }

    /// Sends a move, then gives the worker up to `answerWait` to answer, so its offset is drawn in this frame.
    private func deliver(_ payload: Data) {
        sentPlace = pendingPlace ?? sentPlace
        let before = offsets?.answers.delivered ?? 0
        send?(payload)
        guard answerWait > 0, let offsets else { return }
        let arrived = offsets.answers.wait(after: before, until: Date(timeIntervalSinceNow: answerWait))
        guard arrived else { return }
        MainActor.assumeIsolated { offsets.applyDelivered() }
    }

    @objc private func tick(_ link: CADisplayLink) {
        // The callback's own time, not `link.timestamp`: on the simulator that is a refresh earlier than now, and it
        // must be comparable with the touch's send time.
        if let payload = pacing.frame(at: CACurrentMediaTime()) {
            deliver(payload)
        } else if pacing.pending == nil {
            link.isPaused = true
        }
    }
}
