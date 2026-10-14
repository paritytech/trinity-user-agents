import UIKit
internal import SnapKit

public final class ContactsListViewLayout: DiffableCollectionViewProviderView<String, String> {
    private lazy var separatorConfiguration: SeparatorContentConfiguration = createSeparatorConfiguration()

    public var addToContactsHandler: (() -> Void)?
    public var incomingRequestsHeaderTapHandler: (() -> Void)?

    public var contactSelectionHandler: ((String) -> Void)?

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

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: DSChatListItemConfiguration.defaultReuseIdentifier
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

        var hasRequests: Bool {
            pendingIncomingRequestCount > 0
        }

        public init(
            contactsById: [IdentifiableContentConfiguration<String, DSChatListItemConfiguration>],
            pendingIncomingRequestCount: Int,
            newIncomingRequestCount: Int
        ) {
            self.contactsById = contactsById
            self.pendingIncomingRequestCount = pendingIncomingRequestCount
            self.newIncomingRequestCount = newIncomingRequestCount
        }
    }

    func bind(viewModel: ViewModel) {
        configureView(viewModel: viewModel)
    }
}

private extension ContactsListViewLayout {
    static let incomingRequestCellId: String = "incoming_requests_header"

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

        // Add contacts
        let contactItems = viewModel.contactsById.enumerated().map { offset, item in
            var items: [ItemProviderType] = [
                ItemProviderType(
                    id: item.id,
                    configuration: item.configuration,
                    reuseIdentifier: DSChatListItemConfiguration.defaultReuseIdentifier
                )
            ]
            if offset < viewModel.contactsById.count - 1 {
                let separator = ItemProviderType(
                    id: item.id + "_separator",
                    configuration: separatorConfiguration,
                    reuseIdentifier: SeparatorContentView.reuseIdentifier
                )
                items.append(separator)
            }
            return items
        }.flatMap { $0 }

        itemProviders.append(contentsOf: contactItems)

        let sectionProvider = SectionProviderType(
            id: "ContactsSection",
            itemProviders: itemProviders
        ) { _, _ in
            let group = NSCollectionLayoutGroup.list(
                heightDimension: .estimated(60),
                widthDimension: .fractionalWidth(1.0)
            )

            let section = NSCollectionLayoutSection(group: group)
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

        guard !identifier.hasSuffix("_separator") else { return }

        contactSelectionHandler?(identifier)
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
