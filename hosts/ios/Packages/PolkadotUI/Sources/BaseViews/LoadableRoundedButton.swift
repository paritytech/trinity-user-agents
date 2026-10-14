import UIKit
public import UIKit_iOS

public final class LoadableRoundedButton: ActivityIndicatorLoadableView<RoundedButton> {
    override public var loadingOverlayAnchor: UIView {
        contentView.imageWithTitleView ?? super.loadingOverlayAnchor
    }

    override public func startLoading() {
        if contentView.isEnabled {
            contentView.isEnabled = false
        }
        // trick to start animation after view highlight update
        DispatchQueue.main.async {
            super.startLoading()
        }
    }

    override public func stopLoading() {
        if !contentView.isEnabled {
            contentView.isEnabled = true
        }
        // trick to start animation after view highlight update
        DispatchQueue.main.async {
            super.stopLoading()
        }
    }
}
