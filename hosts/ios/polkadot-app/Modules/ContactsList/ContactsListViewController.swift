import UIKit
import ExternalAccessibility
import PolkadotUI
import UIKit_iOS
import Combine
import FoundationExt

final class ContactsListViewController: UIViewController, ViewHolder, RootScreen {
    typealias RootViewType = ContactsListViewLayout

    let presenter: ContactsListPresenterProtocol
    let scope: ChatListScope
    var timerCancellable: AnyCancellable?
    var isLoading: Bool = true

    private let statusTitleView = NetworkStatusTitleView()
    private lazy var searchController = UISearchController(searchResultsController: nil)

    init(
        presenter: ContactsListPresenterProtocol,
        scope: ChatListScope = .main
    ) {
        self.presenter = presenter
        self.scope = scope
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
        setupSearchField()

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

extension ContactsListViewController: UISearchResultsUpdating {
    func updateSearchResults(for searchController: UISearchController) {
        presenter.search(text: searchController.searchBar.text ?? "")
    }

    /// Puts a text in the search field, as the user types it.
    func setSearchText(_ text: String) {
        searchController.searchBar.text = text
    }
}

private extension ContactsListViewController {
    /// The search of the chat list (07-LATER §8.2). The archived list has none.
    func setupSearchField() {
        guard scope == .main else { return }

        searchController.searchResultsUpdater = self
        searchController.obscuresBackgroundDuringPresentation = false
        searchController.searchBar.placeholder = String(localized: .ChatExtension.listSearchPlaceholder)
        navigationItem.searchController = searchController
        // Under the title, not in the bottom bar: the tab bar of the app covers the bottom.
        navigationItem.preferredSearchBarPlacement = .stacked
        definesPresentationContext = true
    }

    func setupBarItems() {
        guard scope == .main else { return }

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

        rootView.archivedRowTapHandler = { [weak self] in
            self?.presenter.showArchivedChats()
        }

        rootView.trailingSwipeActionsProvider = { [weak self] identifier in
            self?.swipeActions(for: identifier)
        }

        rootView.contextMenuProvider = { [weak self] identifier in
            self?.contextMenu(for: identifier)
        }
    }

    /// Archive and mute, the archive action at the trailing edge (07-LATER §6.2).
    func swipeActions(for identifier: String) -> UISwipeActionsConfiguration? {
        guard let actions = presenter.rowActions(for: identifier) else { return nil }

        let archive = UIContextualAction(
            style: .normal,
            title: ChatMuteOptions.archiveTitle(isArchived: actions.isArchived)
        ) { [weak self] _, _, completion in
            self?.presenter.apply(.archive(!actions.isArchived), contactIdentifier: identifier)
            completion(true)
        }
        archive.image = ChatMuteOptions.archiveImage(isArchived: actions.isArchived)
        archive.backgroundColor = .systemGray

        let mute = UIContextualAction(
            style: .normal,
            title: ChatMuteOptions.muteTitle(isMuted: actions.isMuted)
        ) { [weak self] _, _, completion in
            if actions.isMuted {
                self?.presenter.apply(.unmute, contactIdentifier: identifier)
            } else {
                self?.presenter.requestMute(contactIdentifier: identifier)
            }
            completion(true)
        }
        mute.image = actions.isMuted ? ChatMuteOptions.unmuteImage : ChatMuteOptions.muteImage
        mute.backgroundColor = .systemOrange

        let configuration = UISwipeActionsConfiguration(actions: [archive, mute])
        configuration.performsFirstActionWithFullSwipe = false
        return configuration
    }

    func contextMenu(for identifier: String) -> UIMenu? {
        guard let actions = presenter.rowActions(for: identifier) else { return nil }

        let apply: (ChatRoomUserStateChange) -> Void = { [weak self] change in
            self?.presenter.apply(change, contactIdentifier: identifier)
        }
        return UIMenu(children: [
            ChatMuteOptions.menuElement(isMuted: actions.isMuted, apply: apply),
            ChatMuteOptions.archiveAction(isArchived: actions.isArchived, apply: apply)
        ])
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
