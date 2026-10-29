import UIKit
import FoundationExt
import PolkadotUI

/// Composes the scan panel's content so Common/QRScanner stays free of contact-search knowledge.
final class ScanPanelViewController: UIViewController, ViewHolder {
    typealias RootViewType = ScanPanelViewLayout

    private let scannerController: UIViewController & ScanPanelScannerControlling
    private let presenter: SearchContactPresenterProtocol
    private var isSearchFocused = false
    private var isSearchPending = false
    private var isClosing = false

    var onChatFound: ((ChatOpenModel) -> Void)?
    var onContentHeightChanged: (() -> Void)?
    var onSearchFocusChanged: ((Bool) -> Void)?

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

    /// Suspends capture before the search is cancelled, so the focus change that cancelling causes does not
    /// restart it while the panel closes.
    /// The chrome may call it twice for one close; the second call does nothing.
    func prepareForClose() {
        guard !isClosing else {
            return
        }
        isClosing = true
        scannerController.setCaptureActive(false)
        cancelSearch()
    }

    /// Shows the Search state's layout without focus while a swipe carries the panel toward it.
    /// The bar keeps the Scan state's look until focus.
    func setSearchPending(_ pending: Bool) {
        guard pending != isSearchPending, !isSearchFocused else {
            return
        }
        isSearchPending = pending
        if pending {
            scannerController.setCaptureActive(false)
        }
        let layout = rootView
        DSTabBarStyle.animateCenterChange(in: view) {
            layout.setSearchFocused(pending)
        }
        onContentHeightChanged?()
    }

    /// Focuses the search field, which takes the panel into the Search state.
    func focusSearch() {
        rootView.searchRow.searchField.becomeFirstResponder()
    }

    /// The height this panel declares. The tab bar chrome reads it whenever it sizes the open panel.
    func declaredContentHeight() -> TabBarCenterContentHeight {
        guard !isSearchFocused, !isSearchPending else {
            return .fill
        }
        return .fits(ScanPanelViewLayout.declaredHeight)
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

        let searchField = rootView.searchRow.searchField
        searchField.addTarget(self, action: #selector(searchFieldEditingBegan), for: .editingDidBegin)
        searchField.addTarget(self, action: #selector(searchFieldEditingEnded), for: .editingDidEnd)
    }

    @objc func searchFieldEditingBegan() {
        applySearchFocus(true)
    }

    @objc func searchFieldEditingEnded() {
        applySearchFocus(false)
    }

    /// Collapses the camera and reveals the results inside the same animation as the panel's height change,
    /// which `onContentHeightChanged` drives in the chrome.
    func applySearchFocus(_ focused: Bool) {
        guard focused != isSearchFocused else {
            return
        }
        isSearchFocused = focused
        scannerController.setCaptureActive(!focused && !isClosing)

        let wasPending = isSearchPending
        isSearchPending = false
        let layout = rootView
        DSTabBarStyle.animateCenterChange(in: view) {
            if !(focused && wasPending) {
                layout.setSearchFocused(focused)
            }
        }
        onContentHeightChanged?()
        onSearchFocusChanged?(focused)
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

extension ScanPanelViewController {
    func setCaptureActive(_ active: Bool) {
        // A panel reopened mid-close is the same instance, so opening ends the close.
        if active {
            isClosing = false
        }
        // Search keeps the camera off; ending focus restarts it.
        scannerController.setCaptureActive(active && !isSearchFocused && !isSearchPending)
    }
}
