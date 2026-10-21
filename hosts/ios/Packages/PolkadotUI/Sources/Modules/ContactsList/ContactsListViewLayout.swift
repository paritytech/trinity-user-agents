import UIKit
internal import SnapKit

public final class ContactsListViewLayout: DiffableCollectionViewProviderView<String, String> {
    private lazy var separatorConfiguration: SeparatorContentConfiguration = createSeparatorConfiguration()

    public var addToContactsHandler: (() -> Void)?
    public var incomingRequestsHeaderTapHandler: (() -> Void)?

    public var contactSelectionHandler: ((String) -> Void)?
    public var archivedRowTapHandler: (() -> Void)?
    /// The swipe actions of the row of a chat, by the identifier of the chat. Nil gives the row no actions.
    public var trailingSwipeActionsProvider: ((String) -> UISwipeActionsConfiguration?)?
    /// The menu of a long press on the row of a chat, by the identifier of the chat. Nil gives no menu.
    public var contextMenuProvider: ((String) -> UIMenu?)?

    override public init(frame: CGRect) {
        super.init(frame: frame)
    }

    @available(*, unavailable)
    @MainActor required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func setupViews() {
        backgroundColor = .bgSurfaceMain
        collectionView.backgroundColor = .clear
        collectionView.delegate = self

        addSubview(collectionView)
        collectionView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }
    }

    override public func registerCells() {
        super.registerCells()

        // A list cell, so that the row takes the swipe actions.
        CollectionRegistration.registerCell(
            UICollectionViewListCell.self,
            for: collectionView,
            reuseId: DSChatListItemConfiguration.defaultReuseIdentifier
        )

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: ChatListArchivedRowConfiguration.defaultReuseIdentifier
        )

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: ChatListSectionTitleConfiguration.defaultReuseIdentifier
        )

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: SeparatorContentView.reuseIdentifier
        )

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: IncomingRequestsHeaderView.reuseIdentifier
        )
    }
}

public extension ContactsListViewLayout {
    struct ViewModel {
        let contactsById: [IdentifiableContentConfiguration<String, DSChatListItemConfiguration>]
        let pendingIncomingRequestCount: Int
        let newIncomingRequestCount: Int
        /// The row that opens the archived rooms. Nil shows no row.
        let archivedRow: ChatListArchivedRowConfiguration?
        /// The messages that a search found, under their own title. Empty shows no section.
        let messageHits: MessageHits?

        var hasRequests: Bool {
            pendingIncomingRequestCount > 0
        }

        public init(
            contactsById: [IdentifiableContentConfiguration<String, DSChatListItemConfiguration>],
            pendingIncomingRequestCount: Int,
            newIncomingRequestCount: Int,
            archivedRow: ChatListArchivedRowConfiguration? = nil,
            messageHits: MessageHits? = nil
        ) {
            self.contactsById = contactsById
            self.pendingIncomingRequestCount = pendingIncomingRequestCount
            self.newIncomingRequestCount = newIncomingRequestCount
            self.archivedRow = archivedRow
            self.messageHits = messageHits
        }
    }

    /// The messages that the search of the list found. The identifier of a hit goes to
    /// ``ContactsListViewLayout/contactSelectionHandler`` as the identifier of a row.
    struct MessageHits {
        let title: String
        let hits: [IdentifiableContentConfiguration<String, DSChatListItemConfiguration>]

        public init(
            title: String,
            hits: [IdentifiableContentConfiguration<String, DSChatListItemConfiguration>]
        ) {
            self.title = title
            self.hits = hits
        }
    }

    func bind(viewModel: ViewModel) {
        configureView(viewModel: viewModel)
    }
}

private extension ContactsListViewLayout {
    static let incomingRequestCellId: String = "incoming_requests_header"
    static let archivedRowCellId: String = "archived_chats_row"
    static let messageHitsTitleCellId: String = "message_hits_title"

    // Aligns separator with the DSChatListItem text column:
    //   12pt outer padding + 64pt avatar + 12pt avatar↔content spacing = 88pt
    static let chatListSeparatorLeading: CGFloat = 88
    static let chatListSeparatorTrailing: CGFloat = 12

    func createSeparatorConfiguration() -> SeparatorContentConfiguration {
        .init(
            color: .strokePrimary,
            height: 1,
            insets: NSDirectionalEdgeInsets(
                top: 0,
                leading: Self.chatListSeparatorLeading,
                bottom: 0,
                trailing: Self.chatListSeparatorTrailing
            )
        )
    }

    func configureView(viewModel: ViewModel) {
        var itemProviders: [ItemProviderType] = []

        // Add header if there are incoming requests
        if viewModel.pendingIncomingRequestCount > 0 {
            let headerConfiguration = IncomingRequestsHeaderConfiguration(
                requestCount: viewModel.newIncomingRequestCount
            )
            let headerItem = ItemProviderType(
                id: Self.incomingRequestCellId,
                configuration: headerConfiguration,
                reuseIdentifier: IncomingRequestsHeaderView.reuseIdentifier
            )
            itemProviders.append(headerItem)
        }

        if let archivedRow = viewModel.archivedRow {
            itemProviders.append(ItemProviderType(
                id: Self.archivedRowCellId,
                configuration: archivedRow,
                reuseIdentifier: ChatListArchivedRowConfiguration.defaultReuseIdentifier
            ))
        }

        itemProviders.append(contentsOf: chatItems(viewModel.contactsById))

        if let messageHits = viewModel.messageHits, !messageHits.hits.isEmpty {
            itemProviders.append(ItemProviderType(
                id: Self.messageHitsTitleCellId,
                configuration: ChatListSectionTitleConfiguration(title: messageHits.title),
                reuseIdentifier: ChatListSectionTitleConfiguration.defaultReuseIdentifier
            ))
            itemProviders.append(contentsOf: chatItems(messageHits.hits))
        }

        let sectionProvider = SectionProviderType(
            id: "ContactsSection",
            itemProviders: itemProviders
        ) { [weak self] _, environment in
            var configuration = UICollectionLayoutListConfiguration(appearance: .plain)
            configuration.showsSeparators = false
            configuration.backgroundColor = .clear
            configuration.trailingSwipeActionsConfigurationProvider = { indexPath in
                self?.trailingSwipeActions(at: indexPath)
            }

            let section = NSCollectionLayoutSection.list(using: configuration, layoutEnvironment: environment)
            section.interGroupSpacing = 0
            let topInset: CGFloat = viewModel.hasRequests ? 8 : 16
            section.contentInsets = .init(top: topInset, leading: 0, bottom: 0, trailing: 0)
            return section
        }
        applySnapshot(sections: [
            sectionProvider
        ])
    }

    @objc func handleHeaderTap() {
        addToContactsHandler?()
    }

    /// The rows of chats, with a separator between two rows.
    func chatItems(
        _ chats: [IdentifiableContentConfiguration<String, DSChatListItemConfiguration>]
    ) -> [ItemProviderType] {
        chats.enumerated().flatMap { offset, item in
            var items: [ItemProviderType] = [
                ItemProviderType(
                    id: item.id,
                    configuration: item.configuration,
                    reuseIdentifier: DSChatListItemConfiguration.defaultReuseIdentifier,
                    extraConfigure: { cell in
                        (cell as? UICollectionViewListCell)?.backgroundConfiguration = .clear()
                    }
                )
            ]
            if offset < chats.count - 1 {
                items.append(ItemProviderType(
                    id: item.id + "_separator",
                    configuration: separatorConfiguration,
                    reuseIdentifier: SeparatorContentView.reuseIdentifier
                ))
            }
            return items
        }
    }

    /// The identifier of the chat of a row, or nil for a row that is not a chat.
    func chatIdentifier(at indexPath: IndexPath) -> String? {
        guard let identifier = dataSource.itemIdentifier(for: indexPath) else { return nil }

        let otherRows = [Self.incomingRequestCellId, Self.archivedRowCellId, Self.messageHitsTitleCellId]
        guard !otherRows.contains(identifier), !identifier.hasSuffix("_separator") else { return nil }
        return identifier
    }

    func trailingSwipeActions(at indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        chatIdentifier(at: indexPath).flatMap { trailingSwipeActionsProvider?($0) }
    }
}

extension ContactsListViewLayout: UICollectionViewDelegate {
    public func collectionView(_ collectionView: UICollectionView, didHighlightItemAt indexPath: IndexPath) {
        collectionView.cellForItem(at: indexPath)?.contentView.backgroundColor = .bgSelectionContainerHover
    }

    public func collectionView(_ collectionView: UICollectionView, didUnhighlightItemAt indexPath: IndexPath) {
        guard let cell = collectionView.cellForItem(at: indexPath) else { return }
        UIView.animate(withDuration: 0.2) {
            cell.contentView.backgroundColor = .clear
        }
    }

    public func collectionView(_: UICollectionView, didSelectItemAt indexPath: IndexPath) {
        guard let identifier = dataSource.itemIdentifier(for: indexPath) else {
            return
        }

        if identifier == Self.incomingRequestCellId {
            incomingRequestsHeaderTapHandler?()
            return
        }

        if identifier == Self.archivedRowCellId {
            archivedRowTapHandler?()
            return
        }

        guard let chatIdentifier = chatIdentifier(at: indexPath) else { return }

        contactSelectionHandler?(chatIdentifier)
    }

    public func collectionView(
        _: UICollectionView,
        contextMenuConfigurationForItemsAt indexPaths: [IndexPath],
        point _: CGPoint
    ) -> UIContextMenuConfiguration? {
        guard
            indexPaths.count == 1,
            let chatIdentifier = indexPaths.first.flatMap(chatIdentifier(at:)),
            let menu = contextMenuProvider?(chatIdentifier)
        else {
            return nil
        }

        return UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { _ in menu }
    }
}

#if DEBUG
    #Preview {
        let layout = ContactsListViewLayout()
        let contacts = [
            DSChatListItemConfiguration(
                dateFormatter: TimestampFormatter(),
                avatarViewModel: .colored(text: "J", colorSeed: "jake"),
                sender: "Jake.23",
                message: "You send $71",
                date: .now,
                hasReaction: true,
                unreadCount: 3
            ),
            DSChatListItemConfiguration(
                dateFormatter: TimestampFormatter(),
                avatarViewModel: .colored(text: "M", colorSeed: "max"),
                sender: "Max.12",
                message: "Hello! What can I do for you today?",
                date: .now
            )
        ]
        let viewModel = ContactsListViewLayout.ViewModel(
            contactsById: contacts.identifiedByUUIDs(),
            pendingIncomingRequestCount: 1,
            newIncomingRequestCount: 1
        )
        layout.bind(viewModel: viewModel)
        return layout
    }
#endif
