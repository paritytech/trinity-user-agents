import DesignSystem
import UIKit
internal import UIKit_iOS
internal import SnapKit

final class RemoveDeviceButtonView: UIControl {
    private let iconView: UIImageView = create {
        $0.image = UIImage(resource: .linkedDeviceTrash)
        $0.tintColor = .fgError
        $0.contentMode = .scaleAspectFit
        $0.isUserInteractionEnabled = false
    }

    private let titleLabel: Label = create {
        $0.typography = .bodyLarge
        $0.textColor = .fgError
        $0.text = String(localized: .linkedDevicesDeviceDetailsRemove)
        $0.numberOfLines = 1
        $0.isUserInteractionEnabled = false
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        backgroundColor = .bgSurfaceContainer
        layer.cornerRadius = 16

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension RemoveDeviceButtonView {
    func setupLayout() {
        addSubview(iconView)
        iconView.snp.makeConstraints {
            $0.leading.equalToSuperview().inset(16)
            $0.centerY.equalToSuperview()
            $0.size.equalTo(24)
        }

        addSubview(titleLabel)
        titleLabel.snp.makeConstraints {
            $0.leading.equalTo(iconView.snp.trailing).offset(16)
            $0.centerY.equalToSuperview()
            $0.trailing.lessThanOrEqualToSuperview().inset(16)
        }
    }
}
