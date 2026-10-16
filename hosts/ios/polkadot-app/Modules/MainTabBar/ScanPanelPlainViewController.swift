import UIKit
import FoundationExt

/// Composes the scan panel's content so Common/QRScanner stays free of contact-search knowledge.
final class ScanPanelPlainViewController: UIViewController, ViewHolder {
    typealias RootViewType = ScanPanelPlainViewLayout

    private let scannerController: UIViewController

    var onSearchTap: (() -> Void)?
    var onPanelDragChanged: ((CGFloat) -> Void)?
    var onPanelDragEnded: ((CGFloat) -> Void)?

    init(scannerController: UIViewController) {
        self.scannerController = scannerController
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ScanPanelPlainViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        addChild(scannerController)
        rootView.setupScannerView(scannerController.view)
        scannerController.didMove(toParent: self)

        rootView.searchButton.onTap = { [weak self] in
            self?.onSearchTap?()
        }

        rootView.grabber.onDragChanged = { [weak self] translation in
            self?.onPanelDragChanged?(translation)
        }

        rootView.grabber.onDragEnded = { [weak self] translation in
            self?.onPanelDragEnded?(translation)
        }
    }
}
