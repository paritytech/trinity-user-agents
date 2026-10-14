import DesignSystem
import UIKit

public final class DSLoadableButtonView: ActivityIndicatorLoadableView<DSButtonView> {
    override public init(
        contentView: DSButtonView = DSButtonView(),
        indicatorView: UIActivityIndicatorView = UIActivityIndicatorView()
    ) {
        super.init(contentView: contentView, indicatorView: indicatorView)
        indicatorView.color = .fgPrimary
    }

    override public func startLoading() {
        contentView.isEnabled = false
        super.startLoading()
    }

    override public func stopLoading() {
        contentView.isEnabled = true
        super.stopLoading()
    }
}
