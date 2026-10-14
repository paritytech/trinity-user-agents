import UIKit
import ExternalAccessibility
import PolkadotUI
import UIKit_iOS
import Combine
import FoundationExt

final class ContactsListViewController: UIViewController, ViewHolder, RootScreen {
    typealias RootViewType = ContactsListViewLayout

    let presenter: ContactsListPresenterProtocol
    var timerCancellable: AnyCancellable?
    var isLoading: Bool = true

    private let statusTitleView = NetworkStatusTitleView()

    init(
        presenter: ContactsListPresenterProtocol
    ) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ContactsListViewLayout()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        presenter.viewWillAppear()
        rootView.collectionView.layoutIfNeeded()
        startUpdateTimer()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        presenter.viewWillDisappear()
        stopUpdateTimer()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        setTitleView(statusTitleView)
        addHandlers()
        setupBarItems()

        presenter.setup()
    }

    override func updateContentUnavailableConfiguration(
        using _: UIContentUnavailableConfigurationState
    ) {
        guard !isLoading else {
            contentUnavailableConfiguration = nil
            return
        }

        if rootView.isEmpty {
            contentUnavailableConfiguration = UIContentUnavailableConfiguration.illustrated(
                image: .chatsEmptyIllustration,
                title: String(localized: .chatsEmptyListTitle),
                subtitle: String(localized: .chatsEmptyListMessage)
            )
        } else {
            contentUnavailableConfiguration = nil
        }
    }

    private func startUpdateTimer() {
        timerCancellable = Timer.publish(
            every: AppConfig.timestampRefreshInterval,
            on: RunLoop.main,
            in: .default
        )
        .autoconnect()
        .prepend(.now)
        .sink { [rootView] _ in
            rootView.updateCells()
        }
    }

    private func stopUpdateTimer() {
        timerCancellable?.cancel()
    }
}

private extension ContactsListViewController {
    func setupBarItems() {
        let action = UIAction { [weak self] _ in
            self?.presenter.showScanPanel()
        }

        navigationItem.rightBarButtonItem = UIBarButtonItem(
            image: .add24,
            primaryAction: action
        )

        navigationItem.rightBarButtonItem?.accessibilityId(AccessibilityID.Chats.newChatButton)
        titleView?.accessibilityId(AccessibilityID.Chats.title)
    }

    func addHandlers() {
        rootView.contactSelectionHandler = { [weak self] id in
            self?.presenter.openChat(contactIdentifier: id)
        }

        rootView.incomingRequestsHeaderTapHandler = { [weak self] in
            self?.presenter.showIncomingRequests()
        }
    }
}

extension ContactsListViewController: ContactsListViewProtocol {
    func didReceive(viewModel: ContactsListViewLayout.ViewModel) {
        isLoading = false
        rootView.bind(viewModel: viewModel)

        setNeedsUpdateContentUnavailableConfiguration()
    }

    func didReceive(titleViewModel: NetworkStatusTitleView.ViewModel) {
        statusTitleView.bind(viewModel: titleViewModel)
    }
}
