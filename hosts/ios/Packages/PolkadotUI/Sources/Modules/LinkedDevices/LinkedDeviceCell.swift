import DesignSystem
import UIKit
internal import UIKit_iOS
internal import SnapKit

final class LinkedDeviceCell: UITableViewCell {
    enum Position {
        case first
        case middle
        case last
        case single
    }

    private let containerView: UIView = create {
        $0.backgroundColor = .bgSurfaceContainer
    }

    private let separatorView: UIView = create {
        $0.backgroundColor = .strokePrimary
    }

    private let iconImageView: UIImageView = create {
        $0.contentMode = .scaleAspectFit
        $0.image = UIImage(resource: .linkedDeviceListItemMonitor)
        $0.tintColor = .fgPrimary
    }

    private let labelsView: TopBottomLabelView = create {
        $0.spacing = 0

        $0.topLabel.numberOfLines = 1
        $0.topLabel.typography = .bodyLarge
        $0.topLabel.textColor = .fgPrimary

        $0.bottomLabel.numberOfLines = 1
        $0.bottomLabel.typography = .bodyMedium
        $0.bottomLabel.textColor = .fgTertiary
    }

    private let disclosureImageView: UIImageView = create {
        $0.contentMode = .scaleAspectFit
        $0.image = UIImage(resource: .linkedDeviceDisclosure)
        $0.tintColor = .fgSecondary
    }

    override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
        super.init(style: style, reuseIdentifier: reuseIdentifier)
        selectionStyle = .none
        backgroundColor = .clear
        contentView.backgroundColor = .clear
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension LinkedDeviceCell {
    func setupLayout() {
        contentView.addSubview(containerView)
        containerView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(16)
            $0.top.bottom.equalToSuperview()
        }

        containerView.addSubview(iconImageView)
        iconImageView.snp.makeConstraints {
            $0.size.equalTo(20)
            $0.leading.equalToSuperview().inset(16)
            $0.centerY.equalToSuperview()
        }

        containerView.addSubview(labelsView)
        labelsView.snp.makeConstraints {
            $0.leading.equalTo(iconImageView.snp.trailing).offset(16)
            $0.top.bottom.equalToSuperview().inset(8)
            $0.centerY.equalToSuperview()
        }

        containerView.addSubview(disclosureImageView)
        disclosureImageView.snp.makeConstraints {
            $0.size.equalTo(18)
            $0.centerY.equalToSuperview()
            $0.leading.greaterThanOrEqualTo(labelsView.snp.trailing).offset(16)
            $0.trailing.equalToSuperview().inset(16)
        }

        containerView.addSubview(separatorView)
        separatorView.snp.makeConstraints {
            $0.leading.equalToSuperview().inset(16)
            $0.trailing.equalToSuperview()
            $0.bottom.equalToSuperview()
            $0.height.equalTo(1.0 / UITraitCollection.current.displayScale)
        }
    }

    func applyPosition(_ position: Position) {
        let cornerRadius: CGFloat = 16

        switch position {
        case .single:
            containerView.layer.cornerRadius = cornerRadius
            containerView.layer.maskedCorners = [
                .layerMinXMinYCorner, .layerMaxXMinYCorner,
                .layerMinXMaxYCorner, .layerMaxXMaxYCorner
            ]
            separatorView.isHidden = true
        case .first:
            containerView.layer.cornerRadius = cornerRadius
            containerView.layer.maskedCorners = [
                .layerMinXMinYCorner, .layerMaxXMinYCorner
            ]
            separatorView.isHidden = false
        case .middle:
            containerView.layer.cornerRadius = 0
            containerView.layer.maskedCorners = []
            separatorView.isHidden = false
        case .last:
            containerView.layer.cornerRadius = cornerRadius
            containerView.layer.maskedCorners = [
                .layerMinXMaxYCorner, .layerMaxXMaxYCorner
            ]
            separatorView.isHidden = true
        }
    }
}

extension LinkedDeviceCell {
    func bind(item: LinkedDevicesViewLayout.DeviceItem, position: Position) {
        labelsView.topLabel.text = item.name
        labelsView.bottomLabel.text = item.subtitle

        if let icon = item.icon {
            iconImageView.image = icon
        }

        applyPosition(position)
    }
}
