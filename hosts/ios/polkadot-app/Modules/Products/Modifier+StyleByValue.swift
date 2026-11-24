import PolkadotUI
import SwiftUI
import TrUAPIHost

/// The modifiers a product styles a node with by value, rather than by design
/// token: exact shadows, offsets, type and motion.
struct NodeStyleByValue {
    var shadow: CustomMessageWidgetNode.Shadow?
    var translate = CGSize.zero
    var font: CustomMessageWidgetNode.TextFont?
    var panFade: CustomMessageWidgetNode.PanFade?
    var shimmer: CustomMessageWidgetNode.Shimmer?
    var spinPeriod: TimeInterval?
    var exitFade: TimeInterval?

    /// Keeps `modifier` if it styles by value; any other modifier is left to the caller.
    mutating func take(_ modifier: Modifier) {
        switch modifier {
        case let .shadow(value):
            shadow = CustomMessageWidgetNode.Shadow(
                color: Color(rgba: value.color),
                offset: CGSize(width: CGFloat(value.x) / 100, height: CGFloat(value.y) / 100),
                blur: CGFloat(value.blur) / 100
            )
        case let .translate(value):
            translate = CGSize(width: CGFloat(value.x) / 100, height: CGFloat(value.y) / 100)
        case let .font(value):
            font = CustomMessageWidgetNode.TextFont(
                size: CGFloat(value.size) / 100,
                lineHeight: CGFloat(value.lineHeight) / 100,
                weight: Int(value.weight),
                color: Color(rgba: value.color),
                letterSpacing: CGFloat(value.letterSpacing) / 100
            )
        case let .fadeWithPan(value):
            panFade = CustomMessageWidgetNode.PanFade(key: value.key, rate: CGFloat(value.ratePermille) / 1_000)
        case let .shimmer(value):
            shimmer = CustomMessageWidgetNode.Shimmer(
                color: Color(rgba: value.color | 0xFF),
                stops: value.stops.map { CGFloat($0) / 255 },
                band: CGFloat(value.band) / 100,
                lead: CGFloat(value.lead) / 100,
                speed: CGFloat(value.speed) / 100,
                rest: TimeInterval(value.restMs) / 1_000
            )
        case let .spin(periodMs):
            spinPeriod = periodMs > 0 ? TimeInterval(periodMs) / 1_000 : nil
        case let .exitFade(millis):
            exitFade = TimeInterval(millis) / 1_000
        default:
            break
        }
    }
}

extension Color {
    /// A colour given by value on the wire: `0xRRGGBBAA`, sRGB, straight alpha.
    init(rgba: UInt32) {
        self.init(
            .sRGB,
            red: Double((rgba >> 24) & 0xFF) / 255,
            green: Double((rgba >> 16) & 0xFF) / 255,
            blue: Double((rgba >> 8) & 0xFF) / 255,
            opacity: Double(rgba & 0xFF) / 255
        )
    }
}
