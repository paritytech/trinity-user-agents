import UIKit
import UIKit_iOS

final class LoadableRoundedButton: ActivityIndicatorLoadableView<RoundedButton> {
    override var loadingOverlayAnchor: UIView {
        contentView.imageWithTitleView ?? super.loadingOverlayAnchor
    }

    override func startLoading() {
        if contentView.isEnabled {
            contentView.isEnabled = false
        }
        // trick to start animation after view highlight update
        DispatchQueue.main.async {
            super.startLoading()
        }
    }

    override func stopLoading() {
        if !contentView.isEnabled {
            contentView.isEnabled = true
        }
        // trick to start animation after view highlight update
        DispatchQueue.main.async {
            super.stopLoading()
        }
    }
}
