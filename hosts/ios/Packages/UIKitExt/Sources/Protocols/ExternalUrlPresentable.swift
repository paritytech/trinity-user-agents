import UIKit

public protocol ExternalUrlPresentable: AnyObject {
    func redirectToSafari(url: URL?)
}

extension ExternalUrlPresentable {
    func redirectToSafari(url: URL?) {
        guard
            let url,
            UIApplication.shared.canOpenURL(url)
        else {
            return
        }

        UIApplication.shared.open(url, options: [:], completionHandler: nil)
    }
}
