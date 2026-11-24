import SwiftUI
import Testing
import UIKit
@testable import PolkadotUI

/// A gesture's press feel under touches sent through `UIApplication.sendEvent`, so the pan recogniser and the gesture
/// surface see them in UIKit's own order (this needs the app host: a package test has no event dispatch), and the
/// drawn box says whether the press started.
@MainActor
@Suite("Press from real touches", .serialized)
struct PressFromRealTouchesTests {
    enum Pan: CustomStringConvertible {
        case horizontal
        case freeWithLocation

        var description: String { self == .horizontal ? "horizontal" : "free with location" }
    }

    @Test(
        "A finger held down grows the box by the feel's press, on an axis pan and a free pan that reports the finger",
        arguments: [Pan.horizontal, .freeWithLocation]
    )
    func fingerDownPresses(pan: Pan) async throws {
        let scene = try PressScene(pan: pan)
        defer { scene.close() }
        let rest = try #require(await scene.boxWidth { _ in true }, "box not drawn")
        let center = try #require(scene.boxCenter())
        let grown = { (width: CGFloat) in width > rest + 5 }
        let back = { (width: CGFloat) in abs(width - rest) <= 2 }

        let finger = try FingerTouch(window: scene.window, at: center)
        // The free location pan takes the finger at the touch-down; an axis pan waits for movement. Either way the
        // touch has reached the recogniser, so the press below is not the view's own touch path alone.
        let recognizerState = try scene.surface().panRecognizer.state
        let held = try #require(await scene.boxWidth(until: grown))
        finger.move(to: CGPoint(x: center.x + 2, y: center.y + 1))
        let moved = try #require(await scene.boxWidth(until: grown))
        finger.lift()
        let lifted = try #require(await scene.boxWidth(until: back))

        print(
            "press \(pan): pan recogniser state after touch-down \(recognizerState.rawValue); " +
                "box width rest \(rest) held \(held) moved \(moved) lifted \(lifted)"
        )
        #expect(recognizerState == (pan == .freeWithLocation ? .began : .possible))
        #expect(grown(held), "held: \(held) pt, rest \(rest) pt")
        #expect(grown(moved), "moved: \(moved) pt, rest \(rest) pt")
        #expect(back(lifted), "lifted: \(lifted) pt, rest \(rest) pt")
    }
}

/// A 300 by 160 window with a 300 by 160 gesture node over a plain 100 by 60 grey box at its leading edge, pressed to
/// 1.2 about the box's centre (so the render shows the 10 points it grows past its trailing edge).
@MainActor
private final class PressScene {
    typealias Pan = PressFromRealTouchesTests.Pan

    let window: UIWindow
    private let host: UIViewController
    private static let grey = 128

    init(pan kind: Pan) throws {
        var props = CustomMessageWidgetNode.GestureProps { _, _, _ in Data() }
        props.key = "press"
        var pan = CustomMessageWidgetNode.PanGesture()
        pan.axis = kind == .horizontal ? .horizontal : .free
        pan.reportsLocation = kind == .freeWithLocation
        props.pan = pan
        var feel = CustomMessageWidgetNode.PanFeel()
        feel.pressScale = 1.2
        props.feel = feel
        let box = CustomMessageWidgetNode(
            content: .box(.init(), children: []),
            modifiers: .init(background: .init(color: Color(white: 128.0 / 255)), width: 100, height: 60)
        )
        let root = CustomMessageWidgetNode(
            content: .gesture(props, children: [box]),
            modifiers: .init(width: 300, height: 160)
        )
        host = UIHostingController(
            rootView: CustomMessageWidgetView(node: root, resolveImage: nil)
                .environment(\.gestureOffsets, GestureOffsets())
                .frame(width: 300, height: 160)
        )
        let frame = CGRect(x: 0, y: 0, width: 300, height: 160)
        if let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene {
            window = UIWindow(windowScene: scene)
            window.frame = frame
        } else {
            window = UIWindow(frame: frame)
        }
        window.backgroundColor = .white
        host.view.backgroundColor = .white
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.frame = window.bounds
        host.view.layoutIfNeeded()
    }

    /// The box's drawn width once `condition` holds, checked every 20 ms for up to 2 s; the last width if it never
    /// does, nil if the box is not drawn.
    func boxWidth(until condition: (CGFloat) -> Bool) async -> CGFloat? {
        var width: CGFloat?
        for _ in 0 ..< 100 {
            host.view.layoutIfNeeded()
            width = boxBounds()?.width
            if let width, condition(width) { return width }
            try? await Task.sleep(for: .milliseconds(20))
        }
        return width
    }

    func boxCenter() -> CGPoint? {
        boxBounds().map { CGPoint(x: $0.midX, y: $0.midY) }
    }

    func close() {
        window.isHidden = true
        window.rootViewController = nil
    }

    func surface() throws -> GestureSurfaceView {
        try #require(Self.find(in: host.view))
    }

    /// The grey box's drawn bounds in window points, read from a render of the window at one pixel a point.
    func boxBounds() -> CGRect? {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let image = UIGraphicsImageRenderer(size: window.bounds.size, format: format).image { context in
            window.layer.render(in: context.cgContext)
        }
        guard let picture = image.cgImage, let data = picture.dataProvider?.data,
              let bytes = CFDataGetBytePtr(data) else { return nil }
        let bytesPerPixel = picture.bitsPerPixel / 8
        var bounds: CGRect?
        for row in 0 ..< picture.height {
            for column in 0 ..< picture.width {
                let pixel = bytes + row * picture.bytesPerRow + column * bytesPerPixel
                guard (0 ..< 3).allSatisfy({ abs(Int(pixel[$0]) - Self.grey) <= 6 }) else { continue }
                let dot = CGRect(x: column, y: row, width: 1, height: 1)
                bounds = bounds?.union(dot) ?? dot
            }
        }
        return bounds
    }

    private static func find(in view: UIView) -> GestureSurfaceView? {
        if let match = view as? GestureSurfaceView { return match }
        return view.subviews.lazy.compactMap { find(in: $0) }.first
    }
}

/// One finger, sent through `UIApplication.sendEvent` as the system sends a real one: a `UITouch` with its window,
/// view, place, phase and digitizer event, in the application's touches event for the window.
@MainActor
private final class FingerTouch {
    private let touch = UITouch()
    private let window: UIWindow

    init(window: UIWindow, at point: CGPoint) throws {
        self.window = window
        let view = try #require(window.hitTest(point, with: nil))
        PrivateCall.object(touch, "setWindow:", window)
        PrivateCall.object(touch, "setView:", view)
        PrivateCall.integer(touch, "setTapCount:", 1)
        PrivateCall.point(touch, "_setLocationInWindow:resetPrevious:", point, reset: true)
        PrivateCall.flag(touch, "_setIsFirstTouchForView:", true)
        if touch.responds(to: Selector(("setGestureView:"))) { PrivateCall.object(touch, "setGestureView:", view) }
        send(.began)
    }

    func move(to point: CGPoint) {
        PrivateCall.point(touch, "_setLocationInWindow:resetPrevious:", point, reset: false)
        send(.moved)
    }

    func lift() { send(.ended) }

    private func send(_ phase: UITouch.Phase) {
        PrivateCall.integer(touch, "setPhase:", phase.rawValue)
        PrivateCall.double(touch, "setTimestamp:", ProcessInfo.processInfo.systemUptime)
        let app = UIApplication.shared
        guard let event = PrivateCall.touchesEvent(app, for: window) else {
            Issue.record("the application has no touches event for the window")
            return
        }
        PrivateCall.plain(event, "_clearTouches")
        if let digitizer = DigitizerEvent.finger(at: touch.location(in: window), phase: phase) {
            PrivateCall.pointer(event, "_setHIDEvent:", digitizer)
            PrivateCall.pointer(touch, "_setHidEvent:", digitizer)
        }
        PrivateCall.objectAndFlag(event, "_addTouch:forDelayedDelivery:", touch, false)
        app.sendEvent(event)
    }
}

/// The few private UIKit methods a synthesised touch needs, called through their implementations.
@MainActor
private enum PrivateCall {
    private typealias PlainMethod = @convention(c) (AnyObject, Selector) -> Void
    private typealias ObjectMethod = @convention(c) (AnyObject, Selector, AnyObject) -> Void
    private typealias ObjectAndFlagMethod = @convention(c) (AnyObject, Selector, AnyObject, Bool) -> Void
    private typealias IntegerMethod = @convention(c) (AnyObject, Selector, Int) -> Void
    private typealias FlagMethod = @convention(c) (AnyObject, Selector, Bool) -> Void
    private typealias DoubleMethod = @convention(c) (AnyObject, Selector, Double) -> Void
    private typealias PointMethod = @convention(c) (AnyObject, Selector, CGPoint, Bool) -> Void
    private typealias PointerMethod = @convention(c) (AnyObject, Selector, UnsafeMutableRawPointer) -> Void
    private typealias EventGetter = @convention(c) (AnyObject, Selector, AnyObject) -> Unmanaged<AnyObject>?

    private static func implementation<Method>(_ target: NSObject, _ name: String, _: Method.Type) -> Method {
        unsafeBitCast(target.method(for: Selector(name)), to: Method.self)
    }

    static func plain(_ target: NSObject, _ name: String) {
        implementation(target, name, PlainMethod.self)(target, Selector(name))
    }

    static func object(_ target: NSObject, _ name: String, _ value: AnyObject) {
        implementation(target, name, ObjectMethod.self)(target, Selector(name), value)
    }

    static func objectAndFlag(_ target: NSObject, _ name: String, _ value: AnyObject, _ flag: Bool) {
        implementation(target, name, ObjectAndFlagMethod.self)(target, Selector(name), value, flag)
    }

    static func integer(_ target: NSObject, _ name: String, _ value: Int) {
        implementation(target, name, IntegerMethod.self)(target, Selector(name), value)
    }

    static func flag(_ target: NSObject, _ name: String, _ value: Bool) {
        implementation(target, name, FlagMethod.self)(target, Selector(name), value)
    }

    static func double(_ target: NSObject, _ name: String, _ value: Double) {
        implementation(target, name, DoubleMethod.self)(target, Selector(name), value)
    }

    static func point(_ target: NSObject, _ name: String, _ point: CGPoint, reset: Bool) {
        implementation(target, name, PointMethod.self)(target, Selector(name), point, reset)
    }

    static func pointer(_ target: NSObject, _ name: String, _ pointer: UnsafeMutableRawPointer) {
        guard target.responds(to: Selector(name)) else { return }
        implementation(target, name, PointerMethod.self)(target, Selector(name), pointer)
    }

    /// The application's touches event for `window`, the one UIKit itself hands to `sendEvent`.
    static func touchesEvent(_ app: UIApplication, for window: UIWindow) -> UIEvent? {
        let selector = Selector(("_touchesEventForWindow:"))
        guard let method = class_getInstanceMethod(UIApplication.self, selector) else { return nil }
        let getter = unsafeBitCast(method_getImplementation(method), to: EventGetter.self)
        return getter(app, selector, window)?.takeUnretainedValue() as? UIEvent
    }
}

/// A one-finger digitizer event from IOKit, which UIKit's gesture environment reads alongside the touch.
private enum DigitizerEvent {
    private typealias CreateHand = @convention(c) (
        CFAllocator?, UInt64, UInt32, UInt32, UInt32, UInt32, UInt32,
        Double, Double, Double, Double, Double, Bool, Bool, UInt32
    ) -> UnsafeMutableRawPointer?
    private typealias CreateFinger = @convention(c) (
        CFAllocator?, UInt64, UInt32, UInt32, UInt32,
        Double, Double, Double, Double, Double, Double, Double, Double, Double, Double, Bool, Bool, UInt32
    ) -> UnsafeMutableRawPointer?
    private typealias Append = @convention(c) (UnsafeMutableRawPointer, UnsafeMutableRawPointer, UInt32) -> Void
    private typealias SetInteger = @convention(c) (UnsafeMutableRawPointer, UInt32, Int) -> Void

    private static let iokit = dlopen("/System/Library/Frameworks/IOKit.framework/IOKit", RTLD_NOW)
    /// `kIOHIDDigitizerTransducerTypeHand`.
    private static let handTransducer: UInt32 = 3
    /// `kIOHIDEventFieldDigitizerIsDisplayIntegrated`.
    private static let displayIntegratedField: UInt32 = 0x000B_0019

    private static func symbol<Function>(_ name: String, _: Function.Type) -> Function? {
        guard let iokit, let address = dlsym(iokit, name) else { return nil }
        return unsafeBitCast(address, to: Function.self)
    }

    static func finger(at point: CGPoint, phase: UITouch.Phase) -> UnsafeMutableRawPointer? {
        guard let createHand = symbol("IOHIDEventCreateDigitizerEvent", CreateHand.self),
              let createFinger = symbol("IOHIDEventCreateDigitizerFingerEventWithQuality", CreateFinger.self),
              let append = symbol("IOHIDEventAppendEvent", Append.self),
              let setInteger = symbol("IOHIDEventSetIntegerValue", SetInteger.self) else { return nil }
        let time = mach_absolute_time()
        let touching = phase == .began || phase == .moved || phase == .stationary
        // Range and touch changes when the finger lands or lifts, a position change when it moves.
        let changes: UInt32 = phase == .moved ? 0x4 : 0x1 | 0x2
        let alloc: CFAllocator? = kCFAllocatorDefault
        let place = (x: Double(point.x), y: Double(point.y))
        guard let hand = createHand(alloc, time, handTransducer, 0, 0, changes, 0, 0, 0, 0, 0, 0, false, touching, 0)
        else { return nil }
        setInteger(hand, displayIntegratedField, 1)
        // Index 1, identity 2, a 5 point radius, full quality and density.
        if let finger = createFinger(
            alloc, time, 1, 2, changes, place.x, place.y, 0, 0, 0, 5, 5, 1, 1, 0, touching, touching, 0
        ) {
            append(hand, finger, 0)
        }
        return hand
    }
}
