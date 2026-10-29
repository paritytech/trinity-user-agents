import UIKit

/// Stacks the floating widgets above the bar and passes its own empty area through to the content.
final class MainTabBarFloatingWidgetStackView: UIStackView {
    override init(frame: CGRect) {
        super.init(frame: frame)

        axis = .vertical
        alignment = .fill
        distribution = .fill
        spacing = 0
        setContentHuggingPriority(.required, for: .vertical)
        setContentCompressionResistancePriority(.required, for: .vertical)
    }

    @available(*, unavailable)
    required init(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hitView = super.hitTest(point, with: event)
        return hitView === self ? nil : hitView
    }
}
