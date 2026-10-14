import UIKit

open class TitleValueHorizontalView<TView: UIView, VView: UIView>: GenericPairValueView<TView, VView> {
    public var titleView: TView { fView }
    public var valueView: VView { sView }

    override init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    private var spacer: UIView?

    public var usesSpacer: Bool {
        get {
            spacer != nil
        }

        set {
            if newValue {
                guard spacer == nil else {
                    return
                }

                let view = UIView()
                spacer = view
                stackView.insertArranged(view: view, before: valueView)
            } else {
                spacer?.removeFromSuperview()
                spacer = nil
            }
        }
    }

    private func setupLayout() {
        titleView.setContentHuggingPriority(.defaultLow, for: .horizontal)
        titleView.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        valueView.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        valueView.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)

        stackView.axis = .horizontal
        stackView.alignment = .center
        stackView.distribution = .fill
    }
}
