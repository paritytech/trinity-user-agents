import UIKit
import ExternalAccessibility
import PolkadotUI
import UIKit_iOS
import Combine
import FoundationExt
import SnapKit

final class ContactsListViewController: UIViewController, ViewHolder, RootScreen, NavigationBarHiding {
    typealias RootViewType = ContactsListViewLayout

    let presenter: ContactsListPresenterProtocol
    var timerCancellable: AnyCancellable?
    var isLoading: Bool = true

    private let openingIndicator = UIActivityIndicatorView(style: .medium)

    var hidesNavigationBar: Bool { true }

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
        addHandlers()
        setupOpeningIndicator()
        setContentScrollView(rootView.collectionView, for: .top)

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
    func setupOpeningIndicator() {
        openingIndicator.hidesWhenStopped = true
        view.addSubview(openingIndicator)
        openingIndicator.snp.makeConstraints { make in
            make.top.equalTo(view.safeAreaLayoutGuide).offset(Constants.openingIndicatorInset)
            make.trailing.equalTo(view.safeAreaLayoutGuide).inset(Constants.openingIndicatorInset)
        }
    }

    func addHandlers() {
        rootView.newChatButton.accessibilityId(AccessibilityID.Chats.newChatButton)
        rootView.newChatHandler = { [weak self] in
            self?.presenter.showScanPanel()
        }

        rootView.contactSelectionHandler = { [weak self] id in
            self?.presenter.openChat(contactIdentifier: id)
        }

        rootView.incomingRequestsHeaderTapHandler = { [weak self] in
            self?.presenter.showIncomingRequests()
        }
    }
}

extension ContactsListViewController: ContactsListViewProtocol {
    func didStartOpeningChat() {
        openingIndicator.startAnimating()
    }

    func didStopOpeningChat() {
        openingIndicator.stopAnimating()
    }

    func didReceive(viewModel: ContactsListViewLayout.ViewModel) {
        isLoading = false
        rootView.bind(viewModel: viewModel)

        setNeedsUpdateContentUnavailableConfiguration()
    }
}

private extension ContactsListViewController {
    enum Constants {
        static let openingIndicatorInset: CGFloat = 16
    }
}
