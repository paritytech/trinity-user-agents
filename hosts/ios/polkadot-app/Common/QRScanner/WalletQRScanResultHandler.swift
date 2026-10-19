import UIKit
import UIKitExt

// Shared WalletQRScanDelegate implementation: dismisses the scanner, then routes
// scanned URLs into the deeplink pipeline and DSFiNVK receipts into W3s pay.
final class WalletQRScanResultHandler {
    weak var view: ControllerBackedProtocol?

    private let dsfinvkRouter: W3sDsfinvkRouting
    private let schemeNormalizer: DeeplinkSchemeNormalizing
    private let linkHandler: DeferredLinkHandling

    init(
        dsfinvkRouter: W3sDsfinvkRouting,
        schemeNormalizer: DeeplinkSchemeNormalizing = DeeplinkSchemeNormalizer(),
        linkHandler: DeferredLinkHandling = DeferredLinkHandler.shared
    ) {
        self.dsfinvkRouter = dsfinvkRouter
        self.schemeNormalizer = schemeNormalizer
        self.linkHandler = linkHandler
    }
}

extension WalletQRScanResultHandler: WalletQRScanDelegate {
    func walletQRScanDidReceiveURL(_ url: URL) {
        // The system opens a web room link in the browser, so the app gives it to its own link handler.
        if ChatRoomLink(url.absoluteString) != nil {
            dismissPresented { [linkHandler] in
                linkHandler.handle(with: url)
            }
            return
        }

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

@MainActor
private extension WalletQRScanResultHandler {
    func dismissPresented(completion: @escaping () -> Void) {
        guard let presented = view?.controller.presentedViewController else {
            completion()
            return
        }
        presented.dismiss(animated: true, completion: completion)
    }
}
