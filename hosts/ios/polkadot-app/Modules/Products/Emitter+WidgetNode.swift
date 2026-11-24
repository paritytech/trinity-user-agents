import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Emitters travel in hundredths of a logical pixel and of a degree, and in milliseconds; the widget draws in points,
/// radians and seconds.
extension Emitter {
    var widgetEmitter: CustomMessageWidgetNode.Emitter {
        CustomMessageWidgetNode.Emitter(
            looks: looks.map(\.widgetLook),
            source: source.widgetSource,
            ratePerSecond: Double(ratePerS),
            ratePer100Points: Double(ratePer100pt),
            life: TimeInterval(lifeMs) / 1_000,
            speed: speed.hundredths,
            angle: Double(angleCentideg) / 100 * .pi / 180,
            spread: Double(spreadCentideg) / 100 * .pi / 180,
            gravity: Double(gravity) / 100,
            spin: spin.hundredths,
            fadeFrom: Double(fadeFromPermille) / 1_000,
            endScale: Double(endScalePermille) / 1_000,
            maxAlive: Int(maxAlive),
            bursts: bursts.map { .init(count: Int($0.count), on: $0.on.widgetTrigger) }
        )
    }
}

private extension ValueRange {
    /// The range in whole units, its ends in order.
    var hundredths: ClosedRange<Double> {
        let ends = (Double(min) / 100, Double(max) / 100)
        return Swift.min(ends.0, ends.1) ... Swift.max(ends.0, ends.1)
    }
}

private extension ParticleLook {
    var widgetLook: CustomMessageWidgetNode.Emitter.Look {
        let points = { (value: UInt32) in CGFloat(value) / 100 }
        switch self {
        case let .circle(diameter, color):
            return .circle(diameter: points(diameter), color: Color(rgba: color))
        case let .rectangle(width, height, color):
            return .rectangle(size: CGSize(width: points(width), height: points(height)), color: Color(rgba: color))
        case let .image(source, width, height):
            // Fetched or refused as an image node's picture is: an inline picture is refused and draws no particle.
            let fetched: CustomMessageWidgetNode.ImageSource? =
                switch source {
                case let .bulletin(cid): .bulletin(cid: cid)
                case let .archive(path): .archive(path: path)
                case .inline: nil
                }
            return .image(fetched, size: CGSize(width: points(width), height: points(height)))
        }
    }
}

private extension EmitSource {
    var widgetSource: CustomMessageWidgetNode.Emitter.Source {
        switch self {
        case let .pan(key): .pan(key)
        case let .point(point): .point(CGPoint(x: CGFloat(point.x) / 100, y: CGFloat(point.y) / 100))
        case .area: .area
        }
    }
}

private extension BurstTrigger {
    var widgetTrigger: CustomMessageWidgetNode.Emitter.Trigger {
        switch self {
        case .appear: .appear
        case let .action(name): .action(name)
        case let .change(value): .change(value)
        }
    }
}
