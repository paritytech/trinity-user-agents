import struct TrUAPIHost.ProductId
import Foundation
import IssueMonitoring
import QuartzCore

@MainActor
struct PocketCardTapReporter {
    typealias FrameWaiter = @MainActor (_ drawn: @escaping @MainActor () -> Void) -> Void

    private let telemetry: HostLoadTelemetry
    private let nextFrame: FrameWaiter

    init(
        telemetry: HostLoadTelemetry,
        nextFrame: @escaping FrameWaiter = NextDrawnFrame.wait
    ) {
        self.telemetry = telemetry
        self.nextFrame = nextFrame
    }

    func reportReused(_ tap: HostOpenTap?, productId: ProductId, hostSessionId: UUID?) {
        guard let tap else { return }

        nextFrame { [telemetry] in
            telemetry.record { origin, now in
                HostLoadEvent.pocketCardTapped(
                    productID: productId.rawValue,
                    outcome: .reused,
                    source: tap.source,
                    durationMilliseconds: now - tap.monotonicMilliseconds,
                    hostSessionID: hostSessionId,
                    origin: origin,
                    monotonicMilliseconds: now
                )
            }
        }
    }
}

@MainActor
enum NextDrawnFrame {
    static func wait(_ drawn: @escaping @MainActor () -> Void) {
        let target = DisplayLinkTarget(drawn)
        let link = CADisplayLink(target: target, selector: #selector(DisplayLinkTarget.tick(_:)))
        link.add(to: .main, forMode: .common)
    }
}

@MainActor
private final class DisplayLinkTarget: NSObject {
    private var drawn: (@MainActor () -> Void)?

    init(_ drawn: @escaping @MainActor () -> Void) {
        self.drawn = drawn
    }

    @objc func tick(_ link: CADisplayLink) {
        link.invalidate()
        drawn?()
        drawn = nil
    }
}
