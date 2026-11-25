import UIKit
import UIKitExt

// Shared WalletQRScanDelegate implementation: dismisses the scanner, then routes
// scanned URLs into the deeplink pipeline and DSFiNVK receipts into W3s pay.
final class WalletQRScanResultHandler {
    weak var view: ControllerBackedProtocol?
    /// Closes a scanner that is embedded rather than presented, such as the tab bar's scan panel.
    var closeEmbeddedScanner: (() -> Void)?

    private let dsfinvkRouter: W3sDsfinvkRouting
    private let schemeNormalizer: DeeplinkSchemeNormalizing

    init(
        dsfinvkRouter: W3sDsfinvkRouting,
        schemeNormalizer: DeeplinkSchemeNormalizing = DeeplinkSchemeNormalizer()
    ) {
        self.dsfinvkRouter = dsfinvkRouter
        self.schemeNormalizer = schemeNormalizer
    }
}

extension WalletQRScanResultHandler: WalletQRScanDelegate {
    func walletQRScanDidReceiveURL(_ url: URL) {
        let normalizedUrl = schemeNormalizer.normalize(url)
        // Dismiss the scanner first; otherwise the launcher modal stacks on top of it.
        dismissPresented {
            UIApplication.shared.open(normalizedUrl)
        }
    }

    func walletQRScanDidReceiveDsfinvkReceipt(_ receipt: W3sDsfinvkReceipt) {
        dismissPresented { [dsfinvkRouter] in
            Task { @MainActor in
                await dsfinvkRouter.route(receipt)
            }
        }
    }
}

private extension WalletQRScanResultHandler {
    @MainActor
    func dismissPresented(completion: @escaping () -> Void) {
        guard let presented = view?.controller.presentedViewController else {
            closeEmbeddedScanner?()
            completion()
            return
        }
        presented.dismiss(animated: true, completion: completion)
    }
}
