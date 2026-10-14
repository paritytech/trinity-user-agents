import UIKit
import DesignSystem
internal import SnapKit

final class DownloadFailureOverlayView: UIView {
    private let dimView: UIView = .create { view in
        view.backgroundColor = .bgSurfaceOverlay
    }

    private let iconView: UIImageView = .create { view in
        view.image = UIImage(resource: .fileNotFound)
        view.tintColor = .fgStaticWhite
        view.contentMode = .scaleAspectFit
    }

    private let messageLabel: Label = .create { view in
        view.text = String(localized: .chatMediaNoLongerAvailable)
        view.textColor = .fgStaticWhite
        view.typography = .titleTiny
        view.textAlignment = .center
        view.numberOfLines = 0
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(dimView)
        addSubview(iconView)
        addSubview(messageLabel)

        dimView.snp.makeConstraints { $0.edges.equalToSuperview() }
        iconView.snp.makeConstraints { make in
            make.centerX.equalToSuperview()
            make.centerY.equalToSuperview().offset(-24)
            make.width.height.equalTo(44)
        }
        messageLabel.snp.makeConstraints { make in
            make.top.equalTo(iconView.snp.bottom).offset(12)
            make.leading.trailing.equalToSuperview().inset(24)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
