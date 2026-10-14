import DesignSystem
import UIKit
internal import UIKit_iOS
internal import SnapKit

public final class DeviceDetailsViewLayout: UIView {
    // MARK: - Info Card

    private let infoCardView: GenericBorderedView<UIStackView> = create {
        $0.backgroundView.applyBackgroundStyle(
            .bgSurfaceContainer,
            cornerRadius: 16
        )
        $0.contentInsets = UIEdgeInsets(top: 0, left: 16, bottom: 0, right: 16)
        $0.contentView.axis = .vertical
    }

    private let deviceRow = DeviceDetailsViewLayout.makeInfoRow()
    private let hostRow = DeviceDetailsViewLayout.makeInfoRow()
    private let addedRow = DeviceDetailsViewLayout.makeInfoRow()

    // MARK: - Action Buttons

    private let removeButton = RemoveDeviceButtonView()

    // MARK: - Actions

    private var removeAction: (() -> Void)?

    // MARK: - Init

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .bgSurfaceMain
        setupLayout()
        setupHandlers()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - Public Models

public extension DeviceDetailsViewLayout {
    struct ViewModel {
        public let deviceValue: String
        public let hostValue: String
        public let addedValue: String

        public init(
            deviceValue: String,
            hostValue: String,
            addedValue: String
        ) {
            self.deviceValue = deviceValue
            self.hostValue = hostValue
            self.addedValue = addedValue
        }
    }
}

// MARK: - Public Binding

public extension DeviceDetailsViewLayout {
    func bind(viewModel: ViewModel) {
        deviceRow.titleView.text = String(localized: .linkedDevicesDeviceDetailsDevice)
        deviceRow.valueView.text = viewModel.deviceValue

        hostRow.titleView.text = String(localized: .linkedDevicesDeviceDetailsHost)
        hostRow.valueView.text = viewModel.hostValue

        addedRow.titleView.text = String(localized: .linkedDevicesDeviceDetailsAdded)
        addedRow.valueView.text = viewModel.addedValue
    }

    func bind(removeAction: @escaping () -> Void) {
        self.removeAction = removeAction
    }
}

// MARK: - Private

private extension DeviceDetailsViewLayout {
    static func makeInfoRow() -> TitleValueHorizontalView<Label, Label> {
        let row: TitleValueHorizontalView<Label, Label> = create { view in
            view.titleView.typography = .bodyLarge
            view.titleView.textColor = .fgPrimary
            view.titleView.numberOfLines = 1
            view.titleView.setContentCompressionResistancePriority(.required, for: .horizontal)

            view.valueView.typography = .bodyLarge
            view.valueView.textColor = .fgTertiary
            view.valueView.numberOfLines = 1
            view.valueView.textAlignment = .right
            view.valueView.lineBreakMode = .byTruncatingTail
            view.valueView.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

            view.spacing = 16
        }
        return row
    }

    static func makeSeparator() -> UIView {
        let separator = UIView()
        separator.backgroundColor = .strokePrimary
        separator.snp.makeConstraints {
            $0.height.equalTo(1.0 / UITraitCollection.current.displayScale)
        }
        return separator
    }

    func setupLayout() {
        let separator1 = Self.makeSeparator()
        let separator2 = Self.makeSeparator()

        let rowContainer = UIStackView(arrangedSubviews: [
            makeRowWrapper(deviceRow),
            separator1,
            makeRowWrapper(hostRow),
            separator2,
            makeRowWrapper(addedRow)
        ])
        rowContainer.axis = .vertical

        infoCardView.contentView.addArrangedSubview(rowContainer)

        addSubview(infoCardView)
        infoCardView.snp.makeConstraints {
            $0.top.equalTo(safeAreaLayoutGuide).offset(16)
            $0.leading.trailing.equalToSuperview().inset(16)
        }

        addSubview(removeButton)
        removeButton.snp.makeConstraints {
            $0.top.equalTo(infoCardView.snp.bottom).offset(16)
            $0.leading.trailing.equalToSuperview().inset(16)
            $0.height.equalTo(UIConstants.actionHeight)
        }
    }

    func makeRowWrapper(_ row: UIView) -> UIView {
        let wrapper = UIView()
        wrapper.addSubview(row)
        row.snp.makeConstraints {
            $0.edges
                .equalToSuperview()
                .inset(UIEdgeInsets(top: 16, left: 0, bottom: 16, right: 0))
        }
        return wrapper
    }

    func setupHandlers() {
        removeButton.addTarget(
            self,
            action: #selector(actionRemove),
            for: .touchUpInside
        )
    }

    @objc func actionRemove() {
        removeAction?()
    }
}
