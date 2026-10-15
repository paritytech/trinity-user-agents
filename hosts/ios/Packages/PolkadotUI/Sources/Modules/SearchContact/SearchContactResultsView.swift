import DesignSystem
import UIKit
internal import SnapKit
internal import UIKit_iOS

public enum SearchContactResultsItemId: Hashable {
    case header(sectionId: String)
    case row(sectionId: String, rowId: String)
    case separator(sectionId: String, rowId: String)
}

public final class SearchContactResultsView: DiffableCollectionViewProviderView<String, SearchContactResultsItemId> {
    private let listContainer = UIView()

    private let noResultsLabel: Label = create {
        $0.numberOfLines = 0
        $0.textAlignment = .center
    }

    private let loadingView = SearchContactLoadingView()

    private let fadeMask = CAGradientLayer()

    private let separatorConfiguration = SeparatorContentConfiguration(
        color: UIColor.strokePrimary,
        height: Constants.separatorHeight,
        insets: NSDirectionalEdgeInsets(top: 0, leading: 64, bottom: 0, trailing: DSSpacings.mediumIncreased)
    )

    public var selectionHandler: ((String) -> Void)?

    private var statusFloorConstraint: Constraint?

    override public init(frame: CGRect) {
        super.init(frame: frame)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func layoutSubviews() {
        super.layoutSubviews()
        updateFadeMaskFrame()
    }

    override public func setupViews() {
        backgroundColor = .clear
        collectionView.backgroundColor = .clear
        collectionView.delegate = self
        collectionView.keyboardDismissMode = .none
        collectionView.contentInset.bottom = Constants.fadeHeight

        noResultsLabel.setHidden(true)
        loadingView.setHidden(true)

        setContentCompressionResistancePriority(.defaultLow, for: .vertical)

        fadeMask.colors = [UIColor.black.cgColor, UIColor.black.cgColor, UIColor.clear.cgColor]
        listContainer.layer.mask = fadeMask

        addSubview(listContainer)
        listContainer.addSubview(collectionView)

        listContainer.snp.makeConstraints { make in
            make.top.leading.trailing.equalToSuperview()
            // Yields when the host collapses the view to zero height.
            make.bottom.equalToSuperview().priority(.high)
        }

        collectionView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addSubview(noResultsLabel)
        addSubview(loadingView)

        noResultsLabel.snp.makeConstraints { make in
            make.center.equalToSuperview()
            make.leading.trailing.equalToSuperview().inset(DSSpacings.large)
        }

        loadingView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        snp.makeConstraints { make in
            statusFloorConstraint = make.height.greaterThanOrEqualTo(Constants.statusHeight)
                .priority(.high).constraint
        }
        statusFloorConstraint?.deactivate()
    }

    override public func registerCells() {
        super.registerCells()

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: SearchContactListConfiguration.defaultReuseIdentifier
        )

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: SearchContactSectionHeaderConfiguration.defaultReuseIdentifier
        )

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: SeparatorContentView.reuseIdentifier
        )
    }

    public func bind(status: StatusViewModel) {
        updateStatusFloor(for: status)
        noResultsLabel.attributedText = status.message
        noResultsLabel.setHidden(status.message == nil)
        loadingView.setLoading(status.showsLoader)
    }

    public func bind(viewModel: ViewModel) {
        bind(status: viewModel.status)

        let sections = viewModel.sections.map(makeSectionProvider(for:))
        applySnapshot(sections: sections)
    }
}

public extension SearchContactResultsView {
    /// `message` is centred text shown instead of rows: the no-recents hint or the failure reason.
    struct StatusViewModel {
        public let message: NSAttributedString?
        public let showsLoader: Bool

        public init(
            message: NSAttributedString? = nil,
            showsLoader: Bool = false
        ) {
            self.message = message
            self.showsLoader = showsLoader
        }
    }

    struct ViewModel {
        public struct Section {
            public let id: String
            public let title: String?
            public let rows: [IdentifiableContentConfiguration<String, SearchContactListConfiguration>]

            public init(
                id: String,
                title: String?,
                rows: [IdentifiableContentConfiguration<String, SearchContactListConfiguration>]
            ) {
                self.id = id
                self.title = title
                self.rows = rows
            }
        }

        let sections: [Section]
        let status: StatusViewModel

        public init(sections: [Section], status: StatusViewModel) {
            self.sections = sections
            self.status = status
        }
    }
}

private extension SearchContactResultsView {
    enum Constants {
        static let statusHeight: CGFloat = 120
        static let sectionTopInset = DSSpacings.mediumIncreased
        static let interItemSpacing = DSSpacings.small
        static let separatorHeight: CGFloat = 1
        static let fadeHeight: CGFloat = 16
    }

    func makeSectionProvider(for section: ViewModel.Section) -> SectionProviderType {
        var itemProviders: [ItemProviderType] = []

        if let title = section.title {
            itemProviders.append(
                ItemProviderType(
                    id: .header(sectionId: section.id),
                    configuration: SearchContactSectionHeaderConfiguration(title: title),
                    reuseIdentifier: SearchContactSectionHeaderConfiguration.defaultReuseIdentifier
                )
            )
        }

        for (offset, row) in section.rows.enumerated() {
            itemProviders.append(
                ItemProviderType(
                    id: .row(sectionId: section.id, rowId: row.id),
                    configuration: row.configuration,
                    reuseIdentifier: SearchContactListConfiguration.defaultReuseIdentifier
                )
            )

            if offset < section.rows.count - 1 {
                itemProviders.append(
                    ItemProviderType(
                        id: .separator(sectionId: section.id, rowId: row.id),
                        configuration: separatorConfiguration,
                        reuseIdentifier: SeparatorContentView.reuseIdentifier
                    )
                )
            }
        }

        return SectionProviderType(
            id: section.id,
            itemProviders: itemProviders
        ) { _, _ in Self.makeSectionLayout() }
    }

    static func makeSectionLayout() -> NSCollectionLayoutSection {
        let group = NSCollectionLayoutGroup.list(
            heightDimension: .estimated(56),
            widthDimension: .fractionalWidth(1.0)
        )

        let section = NSCollectionLayoutSection(group: group)
        section.interGroupSpacing = Constants.interItemSpacing
        section.contentInsets = .init(
            top: Constants.sectionTopInset,
            leading: DSSpacings.mediumIncreased,
            bottom: 0,
            trailing: DSSpacings.mediumIncreased
        )
        return section
    }

    /// The mask sits on the non-scrolling container so the gradient stays at the bottom edge while rows
    /// scroll under it. A layer mask ignores Auto Layout, so its frame follows the container on every resize.
    func updateFadeMaskFrame() {
        let bounds = listContainer.bounds
        guard bounds.height > Constants.fadeHeight else { return }

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fadeMask.frame = bounds
        let fadeStart = (bounds.height - Constants.fadeHeight) / bounds.height
        fadeMask.locations = [0, NSNumber(value: fadeStart), 1]
        CATransaction.commit()
    }

    /// A status replaces the rows, so the view keeps a floor height to centre it in.
    func updateStatusFloor(for status: StatusViewModel) {
        if status.showsLoader || status.message != nil {
            statusFloorConstraint?.activate()
        } else {
            statusFloorConstraint?.deactivate()
        }
    }
}

extension SearchContactResultsView: UICollectionViewDelegate {
    public func collectionView(
        _: UICollectionView,
        shouldSelectItemAt indexPath: IndexPath
    ) -> Bool {
        guard case .row = dataSource.itemIdentifier(for: indexPath) else { return false }
        return true
    }

    public func collectionView(
        _ collectionView: UICollectionView,
        didSelectItemAt indexPath: IndexPath
    ) {
        collectionView.deselectItem(at: indexPath, animated: false)
        guard case let .row(_, rowId) = dataSource.itemIdentifier(for: indexPath) else { return }
        selectionHandler?(rowId)
    }
}

#Preview("2 contacts found") {
    let layout = SearchContactResultsView()
    let contacts = [
        SearchContactListConfiguration(
            userName: "Jake.23",
            avatarViewModel: .colored(text: "J", colorSeed: "jake")
        ),
        SearchContactListConfiguration(
            userName: "Max.12",
            avatarViewModel: .colored(text: "M", colorSeed: "max")
        )
    ]
    let section = SearchContactResultsView.ViewModel.Section(
        id: "contacts",
        title: nil,
        rows: contacts.identifiedByUUIDs()
    )
    let viewModel = SearchContactResultsView.ViewModel(
        sections: [section],
        status: SearchContactResultsView.StatusViewModel()
    )
    layout.bind(viewModel: viewModel)
    return layout
}

#Preview("No search results") {
    let layout = SearchContactResultsView()
    let string = NSAttributedString(string: "No results for\n\"notfoundusername\"")
    let viewModel = SearchContactResultsView.ViewModel(
        sections: [],
        status: SearchContactResultsView.StatusViewModel(message: string)
    )
    layout.bind(viewModel: viewModel)
    return layout
}

#Preview("Loading") {
    let layout = SearchContactResultsView()
    let viewModel = SearchContactResultsView.ViewModel(
        sections: [],
        status: SearchContactResultsView.StatusViewModel(showsLoader: true)
    )
    layout.bind(viewModel: viewModel)
    return layout
}

#Preview("Multiple sections with headers") {
    let layout = SearchContactResultsView()
    let recentContacts = [
        SearchContactListConfiguration(
            userName: "Alice.01",
            avatarViewModel: .colored(text: "A", colorSeed: "alice")
        ),
        SearchContactListConfiguration(
            userName: "Bob.02",
            avatarViewModel: .colored(text: "B", colorSeed: "bob")
        )
    ]
    let otherContacts = [
        SearchContactListConfiguration(
            userName: "Charlie.03",
            avatarViewModel: .colored(text: "C", colorSeed: "charlie")
        ),
        SearchContactListConfiguration(
            userName: "Diana.04",
            avatarViewModel: .colored(text: "D", colorSeed: "diana")
        )
    ]

    let sections = [
        SearchContactResultsView.ViewModel.Section(
            id: "recent",
            title: "Recent",
            rows: recentContacts.identifiedByUUIDs()
        ),
        SearchContactResultsView.ViewModel.Section(
            id: "other",
            title: "Other",
            rows: otherContacts.identifiedByUUIDs()
        )
    ]
    let viewModel = SearchContactResultsView.ViewModel(
        sections: sections,
        status: SearchContactResultsView.StatusViewModel()
    )
    layout.bind(viewModel: viewModel)
    return layout
}

private final class SearchContactLoadingView: UIView {
    private enum Constants {
        static let loadingViewSize = CGFloat(64)
    }

    private let loadingView: LoadingView = create {
        $0.contentBackgroundColor = .clear
        $0.contentSize = .init(width: Constants.loadingViewSize, height: Constants.loadingViewSize)
        $0.indicatorImage = UIImage(resource: .searchingUsername)
        $0.tintColor = .fgPrimary
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setLoading(_ loading: Bool) {
        setHidden(!loading)

        if loading {
            loadingView.startAnimating()
        } else {
            loadingView.stopAnimating()
        }
    }

    private func setupLayout() {
        addSubview(loadingView)

        loadingView.snp.makeConstraints {
            $0.center.equalToSuperview()
            $0.size.equalTo(Constants.loadingViewSize)
        }
    }
}
