import PolkadotUI
import UIKit
import UIKitExt

/// Shows the app's success toast over the screen on top.
@MainActor
protocol FundingToastPresentable: AnyObject {
    func showToast(_ message: String)
}

extension FundingToastPresentable {
    func showToast(_ message: String) {
        UIWindow.topWindow?.rootViewController?.topModalViewController.showToast(message: message, type: .success)
    }

    /// Says a value was copied.
    func showCopied() {
        showToast(String(localized: .Funding.depositCopied))
    }
}
