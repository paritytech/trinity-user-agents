import UIKit
import FoundationExt
import PolkadotUI

/// Composes the scan panel's content so Common/QRScanner stays free of contact-search knowledge.
final class ScanPanelViewController: UIViewController, ViewHolder {
    typealias RootViewType = ScanPanelViewLayout

    private let scannerController: UIViewController & ScanPanelScannerControlling
    private let presenter: SearchContactPresenterProtocol

    var onChatFound: ((ChatOpenModel) -> Void)?
    var onContentHeightChanged: (() -> Void)?
    var onPanelDragChanged: ((CGFloat) -> Void)?
    var onPanelDragEnded: ((CGFloat) -> Void)?

    init(
        scannerController: UIViewController & ScanPanelScannerControlling,
        presenter: SearchContactPresenterProtocol
    ) {
        self.scannerController = scannerController
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ScanPanelViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        addChild(scannerController)
        rootView.setupScannerView(scannerController.view)
        scannerController.didMove(toParent: self)

        setupHandlers()
        presenter.setup()
    }

    func cancelSearch() {
        let searchField = rootView.searchRow.searchField
        searchField.text = nil
        presenter.search(username: "")
        searchField.resignFirstResponder()
    }
}

// MARK: - Private

private extension ScanPanelViewController {
    func setupHandlers() {
        rootView.onCameraTapped = { [weak self] in
            self?.cancelSearch()
        }

        rootView.searchRow.searchHandler = { [weak self] text in
            self?.presenter.search(username: text ?? "")
        }

        rootView.resultsView.selectionHandler = { [weak self] identifier in
            self?.presenter.didSelectContact(identifier: identifier)
        }

        rootView.grabber.onDragChanged = { [weak self] translation in
            self?.onPanelDragChanged?(translation)
        }

        rootView.grabber.onDragEnded = { [weak self] translation in
            self?.onPanelDragEnded?(translation)
        }
    }
}

extension ScanPanelViewController: TabBarKeyboardTrackingContent {
    var isKeyboardInputFocused: Bool {
        rootView.searchRow.searchField.isFirstResponder
    }

    func setKeyboardInputFocused(_ focused: Bool) {
        scannerController.setCaptureActive(!focused)
        rootView.setSearchFocused(focused)
    }
}

extension ScanPanelViewController: SearchContactViewProtocol {
    func didReceive(viewModel: SearchContactResultsView.ViewModel) {
        rootView.resultsView.bind(viewModel: viewModel)
        onContentHeightChanged?()
    }

    func didReceive(status: SearchContactResultsView.StatusViewModel) {
        rootView.resultsView.bind(status: status)
        onContentHeightChanged?()
    }
}
