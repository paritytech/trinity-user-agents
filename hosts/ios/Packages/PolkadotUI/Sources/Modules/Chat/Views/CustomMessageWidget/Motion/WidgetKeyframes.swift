import Foundation

extension CustomMessageWidgetNode.Drive {
    /// The property at `panValue`, read modulo `period` when the pan wraps. Before the first keyframe and past the
    /// last, the property holds their values.
    func value(at panValue: Double, period: Int?) -> Double {
        guard let first = keyframes.first, let last = keyframes.last else { return property.neutralValue }

        let input = period.map { Self.wrapped(panValue, period: Double($0)) } ?? panValue
        guard input > first.at else { return first.value }
        guard input < last.at else { return last.value }

        for (previous, next) in zip(keyframes, keyframes.dropFirst()) where input < next.at {
            switch next.curve {
            case .step:
                return previous.value
            case .linear:
                let progress = (input - previous.at) / (next.at - previous.at)
                return previous.value + (next.value - previous.value) * progress
            }
        }
        return last.value
    }
}

extension CustomMessageWidgetNode.Drive.Property {
    /// The value that leaves a node as it would be without the drive.
    var neutralValue: Double {
        switch self {
        case .scale,
             .opacity:
            1
        case .rotationX,
             .rotationY,
             .rotationZ,
             .translationX,
             .translationY:
            0
        }
    }
}

private extension CustomMessageWidgetNode.Drive {
    /// Into `[0, period)`, so a value below zero wraps from the top.
    static func wrapped(_ value: Double, period: Double) -> Double {
        let remainder = value.truncatingRemainder(dividingBy: period)
        return remainder < 0 ? remainder + period : remainder
    }
}
