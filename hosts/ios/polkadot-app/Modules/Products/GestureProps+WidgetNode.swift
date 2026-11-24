import PolkadotUI
import SwiftUI
import TrUAPIHost

extension GestureProps {
    var nodeProps: CustomMessageWidgetNode.GestureProps {
        var value = CustomMessageWidgetNode.GestureProps { phase, position, fraction in
            let nativePhase: GesturePhase =
                switch phase {
                case .tap: .tap
                case .started: .started
                case .moved: .moved
                case .ended: .ended
                case .cancelled: .cancelled
                case .completed: .completed
                }
            // Positions travel in hundredths of a logical pixel, so a slow drag still reports every frame's move.
            return Data(encodeGestureEvent(event: GestureEvent(
                phase: nativePhase,
                x: Int32(clamping: Int((position.x * 100).rounded())),
                y: Int32(clamping: Int((position.y * 100).rounded())),
                fractionPermille: fraction
            )))
        }
        value.tapAction = tapAction
        value.pan = pan?.nodeProps
        value.accessibilityLabel = accessibilityLabel
        value.isEnabled = enabled ?? true
        value.hapticOnComplete = hapticOnComplete ?? false
        value.key = key
        value.feel = feel?.nodeFeel
        return value
    }
}

extension PanFeel {
    var nodeFeel: CustomMessageWidgetNode.PanFeel {
        var value = CustomMessageWidgetNode.PanFeel()
        value.pressScale = CGFloat(pressScalePermille) / 1_000
        value.pressScaleY = pressScaleYPermille.map { CGFloat($0) / 1_000 }
        value.pressDuration = TimeInterval(pressMs) / 1_000
        value.unpressDuration = TimeInterval(unpressMs) / 1_000
        value.springOmega = CGFloat(springOmegaCenti) / 100
        value.springDamping = CGFloat(springDampingCenti) / 100
        value.approachDuration = TimeInterval(approachMs) / 1_000
        value.approachReference = max(CGFloat(approachReference) / 100, 1)
        value.pop = popPermille.map { CGFloat($0) / 1_000 }
        value.popOffset = CGFloat(popOffset) / 100
        return value
    }
}
