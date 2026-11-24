import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Enter animations and tracks, from the core's units to points, degrees, seconds and shares.
extension Enter {
    var widgetEnter: CustomMessageWidgetNode.Enter {
        CustomMessageWidgetNode.Enter(
            from: from.widgetPose,
            motion: motion.widgetMotion,
            delay: TimeInterval(delayMs) / 1_000
        )
    }
}

extension Pose {
    var widgetPose: CustomMessageWidgetNode.Pose {
        CustomMessageWidgetNode.Pose(
            opacity: Double(opacityPermille) / 1_000,
            scale: Double(scalePermille) / 1_000,
            offset: CGSize(width: CGFloat(x) / 100, height: CGFloat(y) / 100),
            rotation: Double(rotationCentideg) / 100
        )
    }
}

extension Motion {
    var widgetMotion: CustomMessageWidgetNode.Motion {
        switch self {
        case let .timed(milliseconds, curve):
            .timed(duration: TimeInterval(milliseconds) / 1_000, curve: curve.widgetCurve)
        case let .spring(spring):
            .spring(omega: Double(spring.omegaCenti) / 100, damping: Double(spring.dampingCenti) / 100)
        }
    }
}

extension Curve {
    var widgetCurve: CustomMessageWidgetNode.Curve {
        CustomMessageWidgetNode.Curve(
            x1: Double(x1) / 1_000, y1: Double(y1) / 1_000, x2: Double(x2) / 1_000, y2: Double(y2) / 1_000
        )
    }
}

extension Track {
    var widgetTrack: CustomMessageWidgetNode.Track {
        let property = property.widgetProperty
        // Values are scaled into the property's own units here: the scale commutes with every curve and keyframe.
        let unit = property.unit
        switch play {
        case let .keyframes(keyframes):
            return CustomMessageWidgetNode.Track(property: property, play: .keyframes(.init(
                frames: keyframes.frames.map {
                    .init(
                        at: TimeInterval($0.atMs) / 1_000,
                        value: Double($0.value) * unit,
                        curve: $0.curve.widgetCurve
                    )
                },
                runs: Int(keyframes.repeat),
                alternate: keyframes.alternate == true
            )))
        case let .follow(target, motion):
            return CustomMessageWidgetNode.Track(
                property: property, play: .follow(target: Double(target) * unit, motion: motion.widgetMotion)
            )
        }
    }
}

/// The modifiers that move a node on the host's clock, gathered in the order the tree lists them: a node keeps its
/// last enter, emitter and parallax, and its last track per property.
struct NodeMotionByValue {
    private(set) var enter: CustomMessageWidgetNode.Enter?
    private(set) var tracks: [CustomMessageWidgetNode.Track] = []
    private(set) var emitter: CustomMessageWidgetNode.Emitter?
    private(set) var parallax: CustomMessageWidgetNode.Parallax?

    mutating func take(_ modifier: Modifier) {
        switch modifier {
        case let .enter(value):
            enter = value.widgetEnter
        case let .animate(track):
            let mapped = track.widgetTrack
            tracks.removeAll { $0.property == mapped.property }
            tracks.append(mapped)
        case let .emit(value):
            emitter = value.widgetEmitter
        case let .parallax(value):
            parallax = value.widgetParallax
        default:
            break
        }
    }
}

extension AnimatedProperty {
    var widgetProperty: CustomMessageWidgetNode.AnimatedProperty {
        switch self {
        case .opacity: .opacity
        case .scale: .scale
        case .offsetX: .offsetX
        case .offsetY: .offsetY
        case .rotation: .rotation
        }
    }
}

private extension CustomMessageWidgetNode.AnimatedProperty {
    /// One of the core's units in the host's: thousandths for opacity and scale, hundredths of a point or a degree
    /// for offsets and turn.
    var unit: Double {
        switch self {
        case .opacity,
             .scale: 1.0 / 1_000
        case .offsetX,
             .offsetY,
             .rotation: 1.0 / 100
        }
    }
}
