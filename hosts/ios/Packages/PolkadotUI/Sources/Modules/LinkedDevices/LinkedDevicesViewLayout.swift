import DesignSystem
import UIKit
internal import UIKit_iOS
internal import SnapKit

public final class LinkedDevicesViewLayout: UIView {
    // MARK: - Empty State Views

    private let emptyContainerView: UIView = create {
        $0.isHidden = true
    }

    private let emptyImageView: UIImageView = create {
        $0.contentMode = .scaleAspectFit
        $0.tintColor = .fgPrimary
        $0.image = UIImage(resource: .linkedDeviceMonitor)
    }

    private let emptyTitleLabel: Label = create {
        $0.typography = .headlineSmall
        $0.textColor = .fgPrimary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    private let emptySubtitleLabel: Label = create {
        $0.typography = .paragraphLarge
        $0.textColor = .fgSecondary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    private let scanButton: RoundedButton = create {
        $0.applyMainStyle()
    }

    private let footerLabel: Label = create {
        $0.typography = .bodyMedium
        $0.textColor = .fgTertiary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    private let howItWorksButton: RoundedButton = create {
        $0.applyTitleStyle()
        $0.imageWithTitleView?.titleColor = .fgSecondary
        $0.tintColor = .fgSecondary
        $0.imageWithTitleView?.titleFont = .regular14
        $0.imageWithTitleView?.spacingBetweenLabelAndIcon = 0
        $0.imageWithTitleView?.layoutType = .horizontalLabelFirst
        $0.isHidden = true // always hidden for now
    }

    // MARK: - List State Views

    private let tableView: UITableView = {
        let tableView = UITableView(frame: .zero, style: .grouped)
        tableView.registerClassForCell(LinkedDeviceCell.self)
        tableView.registerHeaderFooterView(withClass: LinkedDevicesSectionHeaderView.self)
        tableView.separatorStyle = .none
        tableView.backgroundColor = .bgSurfaceMain
        tableView.delaysContentTouches = false
        tableView.sectionHeaderTopPadding = 0
        tableView.sectionFooterHeight = 0
        tableView.isHidden = true
        return tableView
    }()

    private lazy var dataSource = makeDataSource()

    private var currentSectionHeader: DeviceSectionHeader?

    // MARK: - Actions

    private var scanAction: (() -> Void)?
    private var howItWorksAction: (() -> Void)?
    private var deviceSelectedAction: ((Int) -> Void)?

    // MARK: - Public Controls

    public var scanQRButton: UIControl {
        scanButton
    }

    // MARK: - Init

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .bgSurfaceMain
        setupLayout()
        setupHandlers()
        tableView.delegate = self
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - UITableViewDelegate

extension LinkedDevicesViewLayout: UITableViewDelegate {
    public func tableView(
        _ tableView: UITableView,
        viewForHeaderInSection _: Int
    ) -> UIView? {
        guard let header = currentSectionHeader else { return nil }

        let headerView: LinkedDevicesSectionHeaderView = tableView.dequeueReusableHeaderFooterView()
        headerView.bind(header: header)
        return headerView
    }

    public func tableView(
        _: UITableView,
        heightForHeaderInSection _: Int
    ) -> CGFloat {
        currentSectionHeader != nil ? 40 : 0
    }

    public func tableView(
        _: UITableView,
        didSelectRowAt indexPath: IndexPath
    ) {
        deviceSelectedAction?(indexPath.row)
    }

    public func tableView(
        _: UITableView,
        viewForFooterInSection _: Int
    ) -> UIView? {
        nil
    }

    public func tableView(
        _: UITableView,
        heightForFooterInSection _: Int
    ) -> CGFloat {
        .leastNormalMagnitude
    }
}

// MARK: - Private Layout

private extension LinkedDevicesViewLayout {
    enum Section: Hashable {
        case devices
    }

    typealias DataSource = UITableViewDiffableDataSource<Section, DeviceItem>
    typealias Snapshot = NSDiffableDataSourceSnapshot<Section, DeviceItem>

    func makeDataSource() -> DataSource {
        let dataSource = DataSource(tableView: tableView) { tableView, indexPath, item in
            let cell = tableView.dequeueReusableCellWithType(LinkedDeviceCell.self)
            let totalRows = tableView.numberOfRows(inSection: indexPath.section)
            let position = Self.cellPosition(forRow: indexPath.row, totalRows: totalRows)
            cell?.bind(item: item, position: position)
            return cell
        }
        dataSource.defaultRowAnimation = .fade
        return dataSource
    }

    static func cellPosition(forRow row: Int, totalRows: Int) -> LinkedDeviceCell.Position {
        if totalRows == 1 {
            .single
        } else if row == 0 {
            .first
        } else if row == totalRows - 1 {
            .last
        } else {
            .middle
        }
    }

    func setupLayout() {
        setupEmptyStateLayout()
        setupTableLayout()
    }

    func setupEmptyStateLayout() {
        addSubview(emptyContainerView)
        emptyContainerView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        let contentStack = UIStackView(arrangedSubviews: [
            emptyImageView,
            emptyTitleLabel,
            emptySubtitleLabel,
            scanButton
        ])
        contentStack.axis = .vertical
        contentStack.alignment = .center
        contentStack.setCustomSpacing(24, after: emptyImageView)
        contentStack.setCustomSpacing(8, after: emptyTitleLabel)
        contentStack.setCustomSpacing(24, after: emptySubtitleLabel)

        emptyImageView.snp.makeConstraints {
            $0.size.equalTo(80)
        }

        scanButton.snp.makeConstraints {
            $0.height.equalTo(UIConstants.actionHeight)
            $0.width.greaterThanOrEqualTo(175)
        }

        let bottomStack = UIStackView(arrangedSubviews: [footerLabel, howItWorksButton])
        bottomStack.axis = .vertical
        bottomStack.alignment = .center
        bottomStack.spacing = 24

        emptyContainerView.addSubview(bottomStack)
        bottomStack.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(24)
            $0.bottom.equalTo(safeAreaLayoutGuide).inset(24)
        }

        howItWorksButton.snp.makeConstraints {
            $0.height.equalTo(UIConstants.actionHeight)
        }

        let layoutGuide = UILayoutGuide()
        addLayoutGuide(layoutGuide)
        layoutGuide.snp.makeConstraints {
            $0.top.equalTo(safeAreaLayoutGuide.snp.top)
            $0.bottom.equalTo(bottomStack.snp.top)
        }

        emptyContainerView.addSubview(contentStack)
        contentStack.snp.makeConstraints {
            $0.centerY.equalTo(layoutGuide.snp.centerY)
            $0.leading.trailing.equalToSuperview().inset(24)
        }
    }

    func setupTableLayout() {
        addSubview(tableView)
        tableView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }

    func setupHandlers() {
        scanButton.addTarget(
            self,
            action: #selector(actionScan),
            for: .touchUpInside
        )

        howItWorksButton.addTarget(
            self,
            action: #selector(actionHowItWorks),
            for: .touchUpInside
        )
    }

    @objc func actionScan() {
        scanAction?()
    }

    @objc func actionHowItWorks() {
        howItWorksAction?()
    }

    func applySnapshot(with items: [DeviceItem]) {
        var snapshot = Snapshot()
        snapshot.appendSections([.devices])
        snapshot.appendItems(items)
        dataSource.apply(snapshot, animatingDifferences: true)
    }
}

// MARK: - Public Models & Binding

public extension LinkedDevicesViewLayout {
    struct DeviceItem: Hashable {
        public let id: String
        public let icon: UIImage?
        public let name: String
        public let subtitle: String

        public init(
            id: String,
            icon: UIImage?,
            name: String,
            subtitle: String
        ) {
            self.id = id
            self.icon = icon
            self.name = name
            self.subtitle = subtitle
        }

        public static func == (lhs: DeviceItem, rhs: DeviceItem) -> Bool {
            lhs.id == rhs.id
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(id)
        }
    }

    struct DeviceSectionHeader {
        public let title: String
        public let count: String

        public init(title: String, count: String) {
            self.title = title
            self.count = count
        }
    }

    enum ViewModel {
        case empty(EmptyViewModel)
        case devices(DevicesViewModel)
    }

    struct EmptyViewModel {
        public let title: String
        public let subtitle: String
        public let scanButtonTitle: String
        public let footerText: String
        public let howItWorksTitle: String

        public init(
            title: String,
            subtitle: String,
            scanButtonTitle: String,
            footerText: String,
            howItWorksTitle: String
        ) {
            self.title = title
            self.subtitle = subtitle
            self.scanButtonTitle = scanButtonTitle
            self.footerText = footerText
            self.howItWorksTitle = howItWorksTitle
        }
    }

    struct DevicesViewModel {
        public let sectionHeader: DeviceSectionHeader?
        public let items: [DeviceItem]

        public init(
            sectionHeader: DeviceSectionHeader?,
            items: [DeviceItem]
        ) {
            self.sectionHeader = sectionHeader
            self.items = items
        }
    }

    func bind(viewModel: ViewModel) {
        switch viewModel {
        case let .empty(emptyModel):
            bindEmpty(emptyModel)
        case let .devices(devicesModel):
            bindDevices(devicesModel)
        }
    }

    func bind(scanAction: @escaping () -> Void) {
        self.scanAction = scanAction
    }

    func bind(howItWorksAction: @escaping () -> Void) {
        self.howItWorksAction = howItWorksAction
    }

    func bind(deviceSelectedAction: @escaping (Int) -> Void) {
        self.deviceSelectedAction = deviceSelectedAction
    }
}

// MARK: - Private Binding

private extension LinkedDevicesViewLayout {
    func bindEmpty(_ model: EmptyViewModel) {
        emptyContainerView.isHidden = false
        tableView.isHidden = true

        emptyTitleLabel.text = model.title
        emptySubtitleLabel.text = model.subtitle
        scanButton.imageWithTitleView?.title = model.scanButtonTitle
        footerLabel.text = model.footerText
        howItWorksButton.imageWithTitleView?.title = model.howItWorksTitle
        howItWorksButton.setIcon(UIImage(resource: .linkedDeviceHowItWorksDisclosure))
        howItWorksButton.invalidateLayout()
    }

    func bindDevices(_ model: DevicesViewModel) {
        emptyContainerView.isHidden = true
        tableView.isHidden = false

        currentSectionHeader = model.sectionHeader
        applySnapshot(with: model.items)
    }
}
