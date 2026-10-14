import DesignSystem
import UIKit
internal import UIKit_iOS
internal import SnapKit

final class LinkedDevicesSectionHeaderView: UITableViewHeaderFooterView {
    private let titleLabel: Label = create {
        $0.typography = .labelMedium.emphasized
        $0.textColor = .fgTertiary
    }

    private let countLabel: Label = create {
        $0.typography = .labelMedium.emphasized
        $0.textColor = .fgTertiary
    }

    override init(reuseIdentifier: String?) {
        super.init(reuseIdentifier: reuseIdentifier)
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension LinkedDevicesSectionHeaderView {
    func setupLayout() {
        let backgroundConfig = UIBackgroundConfiguration.clear()
        backgroundConfiguration = backgroundConfig

        contentView.addSubview(titleLabel)
        titleLabel.snp.makeConstraints {
            $0.leading.equalToSuperview().inset(16)
            $0.centerY.equalToSuperview()
        }

        contentView.addSubview(countLabel)
        countLabel.snp.makeConstraints {
            $0.trailing.equalToSuperview().inset(16)
            $0.centerY.equalToSuperview()
            $0.leading.greaterThanOrEqualTo(titleLabel.snp.trailing).offset(8)
        }
    }
}

extension LinkedDevicesSectionHeaderView {
    func bind(header: LinkedDevicesViewLayout.DeviceSectionHeader) {
        titleLabel.text = header.title
        countLabel.text = header.count
    }
}
