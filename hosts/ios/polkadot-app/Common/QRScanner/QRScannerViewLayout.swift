import UIKit
import UIKit_iOS
import PolkadotUI
import DesignSystem

class QRScannerViewLayout: UIView, AdaptiveDesignable {
    let qrFrameView: CameraFrameView = .create { view in
        view.cornerRadius = 24.0
        view.windowPosition = CGPoint(x: 0.5, y: 0.47)
    }

    let qrFrameImageView: RoundedView = .create { view in
        view.applyBorderStyle(
            .fgStaticWhite,
            strokeWidth: 4,
            cornerRadius: 24
        )
    }

    let titleLabel: Label = .create { (view: Label) in
        view.text = String(localized: .qrScannerTitle)
        view.numberOfLines = 0
        view.textAlignment = .center
        view.typography = .titleMedium
        view.textColor = .fgStaticWhite
    }

    let messageLabel: Label = .create { (view: Label) in
        view.numberOfLines = 0
        view.textAlignment = .center
        view.typography = .titleMedium
        view.textColor = .fgStaticWhite
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        backgroundColor = .bgSurfaceMain

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setupLayout() {
        addSubview(qrFrameView)

        qrFrameView.snp.makeConstraints { make in
            make.left.right.top.bottom.equalToSuperview()
        }

        qrFrameView.addSubview(qrFrameImageView)

        let windowSize = 312.0 * min(designScaleRatio.width, 1)
        qrFrameView.windowSize = CGSize(width: windowSize, height: windowSize)

        qrFrameImageView.snp.makeConstraints { make in
            make.centerX.equalTo(qrFrameView.snp.trailing).multipliedBy(qrFrameView.windowPosition.x)
            make.centerY.equalTo(qrFrameView.snp.bottom).multipliedBy(qrFrameView.windowPosition.y)
            make.size.equalTo(windowSize)
        }

        addSubview(titleLabel)
        titleLabel.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(16)
            make.top.equalTo(qrFrameImageView.snp.bottom).offset(90.0)
        }

        addSubview(messageLabel)
        messageLabel.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(16)
            make.bottom.equalTo(safeAreaLayoutGuide).offset(-24.0)
        }
    }

    /// Called once the preview layer is attached and its session is running. The base layout
    /// shows the preview immediately; subclasses covering the start-up delay fade it in here.
    func didAttachPreview() {}
}
