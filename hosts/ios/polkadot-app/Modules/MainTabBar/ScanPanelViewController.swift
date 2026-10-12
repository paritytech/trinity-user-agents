import UIKit
import FoundationExt
import PolkadotUI

/// Composes the scan panel's content so Common/QRScanner stays free of contact-search knowledge.
final class ScanPanelViewController: UIViewController, ViewHolder {
    typealias RootViewType = ScanPanelViewLayout

    private let scannerController: UIViewController & ScanPanelScannerControlling
    private let presenter: SearchContactPresenterProtocol
    private var isSearchFocused = false

    var onChatFound: ((ChatOpenModel) -> Void)?
    var onContentHeightChanged: (() -> Void)?

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

    /// The height this panel declares at `width`. The tab bar chrome reads it whenever it sizes the open panel.
    func declaredContentHeight(forPanelWidth width: CGFloat) -> TabBarCenterContentHeight {
        guard !isSearchFocused else {
            return .fill
        }
        let height = ScanPanelContentHeight.height(
            forPanelWidth: width,
            searchRowHeight: ScanPanelContentHeight.embeddedSearchRowHeight
        )
        return .fits(height)
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

        let layout = rootView
        DSTabBarStyle.animateCenterChange(in: view) {
            layout.setSearchFocused(focused)
        }
        onContentHeightChanged?()
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
