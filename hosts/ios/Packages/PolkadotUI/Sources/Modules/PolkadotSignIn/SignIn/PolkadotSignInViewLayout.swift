import DesignSystem
import ExternalAccessibility
import UIKit
internal import UIKit_iOS
internal import SnapKit

public final class PolkadotSignInViewLayout: BottomSheetBaseLayout {
    private let indicatorView: UIActivityIndicatorView = {
        let view = UIActivityIndicatorView(style: .medium)
        view.color = .fgPrimary
        return view
    }()

    public let resultView = PolkadotSignInResultView()

    override public func setupLayout() {
        super.setupLayout()

        contentView.addSubview(resultView)
        resultView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        contentView.addSubview(indicatorView)
        indicatorView.snp.makeConstraints {
            $0.centerX.centerY.equalToSuperview()
        }

        resultView.bindStaticContent()
    }
}

public extension PolkadotSignInViewLayout {
    enum ViewModel {
        case inProgress
        case result(PolkadotSignInResultView.ViewModel)
        case sendingHandshake
    }

    func bind(viewModel: ViewModel) {
        switch viewModel {
        case .inProgress:
            indicatorView.setHidden(false)
            indicatorView.startAnimating()
            resultView.alpha = 0
            resultView.showActivity(active: false)
        case let .result(resultViewModel):
            indicatorView.setHidden(true)
            indicatorView.stopAnimating()
            resultView.alpha = 1
            resultView.showActivity(active: false)
            resultView.bind(viewModel: resultViewModel)
            resultView.bindTitle(String(localized: .linkedDevicesSignInTitle))
        case .sendingHandshake:
            indicatorView.setHidden(true)
            indicatorView.stopAnimating()
            resultView.alpha = 1
            resultView.showActivity(active: true)
            resultView.bindTitle(String(localized: .linkedDevicesSignInConnecting))
        }
    }
}

public final class PolkadotSignInResultView: UIView {
    private let titleLabel: Label = create {
        $0.typography = .headlineSmall
        $0.textColor = .fgPrimary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    private let iconImageView: UIImageView = create {
        $0.contentMode = .scaleAspectFit
        $0.image = UIImage(resource: .linkedDeviceMonitor)
        $0.tintColor = .fgPrimary
    }

    private let deviceDescriptionLabel: Label = create {
        $0.typography = .paragraphLarge
        $0.textColor = .fgPrimary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    private let infoCardView: UIView = create {
        $0.backgroundColor = .bgSurfaceNested
        $0.layer.cornerRadius = 16
    }

    private let infoHeaderLabel: Label = create {
        $0.typography = .bodyMedium
        $0.textColor = .fgPrimary
        $0.numberOfLines = 0
    }

    private let bulletStackView: UIStackView = create {
        $0.axis = .vertical
        $0.spacing = 8
    }

    private let actionView: GenericPairValueView<
        RoundedButton,
        LoadableRoundedButton
    > = .create { view in
        view.fView.applySecondaryStyle()
        view.fView.setTitle(String(localized: .Common.cancel))

        view.sView.contentView.applyMainStyle()
        view.sView.contentView.setTitle(String(localized: .linkedDevicesSignInLink))
        view.sView.indicatorView.color = .fgPrimaryInverted

        view.setHorizontalAndSpacing(16)
        view.stackView.distribution = .fillEqually
    }

    public var cancelButton: UIControl {
        actionView.fView
    }

    public var linkButton: UIControl {
        actionView.sView.contentView
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension PolkadotSignInResultView {
    func setupLayout() {
        addSubview(titleLabel)
        titleLabel.snp.makeConstraints {
            $0.top.equalToSuperview().offset(16)
            $0.leading.trailing.equalToSuperview()
        }

        addSubview(iconImageView)
        iconImageView.snp.makeConstraints {
            $0.top.equalTo(titleLabel.snp.bottom).offset(48)
            $0.centerX.equalToSuperview()
            $0.height.equalTo(68)
            $0.width.equalTo(80)
        }

        addSubview(deviceDescriptionLabel)
        deviceDescriptionLabel.snp.makeConstraints {
            $0.top.equalTo(iconImageView.snp.bottom).offset(16)
            $0.leading.trailing.equalToSuperview()
        }

        addSubview(infoCardView)
        infoCardView.snp.makeConstraints {
            $0.top.equalTo(deviceDescriptionLabel.snp.bottom).offset(48)
            $0.leading.trailing.equalToSuperview()
        }

        infoCardView.addSubview(infoHeaderLabel)
        infoHeaderLabel.snp.makeConstraints {
            $0.top.leading.trailing.equalToSuperview().inset(24)
        }

        infoCardView.addSubview(bulletStackView)
        bulletStackView.snp.makeConstraints {
            $0.top.equalTo(infoHeaderLabel.snp.bottom).offset(16)
            $0.leading.trailing.equalToSuperview().inset(24)
            $0.bottom.equalToSuperview().inset(24)
        }

        addSubview(actionView)
        actionView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview()
            $0.bottom.equalToSuperview().inset(16)
            $0.top.equalTo(infoCardView.snp.bottom).offset(48)
        }

        actionView.sView.contentView.snp.makeConstraints {
            $0.height.equalTo(UIConstants.actionHeight)
        }

        setupBulletItems()
    }

    func setupBulletItems() {
        let bullets = [
            String(localized: .linkedDevicesSignInBulletChatsAvailable),
            String(localized: .linkedDevicesSignInBulletSendReceive),
            String(localized: .linkedDevicesSignInBulletRemoveAnytime)
        ]

        for bullet in bullets {
            let bulletView = makeBulletItem(text: bullet)
            bulletStackView.addArrangedSubview(bulletView)
        }
    }

    func makeBulletItem(text: String) -> UIView {
        let container = UIView()

        let dot = UIView()
        dot.backgroundColor = .fgSecondary
        dot.layer.cornerRadius = 3

        let label = Label()
        label.typography = .bodyMedium
        label.textColor = .fgSecondary
        label.numberOfLines = 0
        label.text = text

        container.addSubview(dot)
        dot.snp.makeConstraints {
            $0.leading.equalToSuperview()
            $0.top.equalToSuperview().offset(8)
            $0.size.equalTo(6)
        }

        container.addSubview(label)
        label.snp.makeConstraints {
            $0.leading.equalTo(dot.snp.trailing).offset(8)
            $0.trailing.top.bottom.equalToSuperview()
        }

        return container
    }
}

public extension PolkadotSignInResultView {
    struct ViewModel {
        let deviceDescription: String

        public init(deviceDescription: String) {
            self.deviceDescription = deviceDescription
        }
    }

    func bindStaticContent() {
        titleLabel.text = String(localized: .linkedDevicesSignInTitle)
        deviceDescriptionLabel.text = " \n "
        infoHeaderLabel.text = String(localized: .linkedDevicesSignInInfoHeader)
    }

    func bindTitle(_ title: String) {
        titleLabel.text = title
    }

    func bind(viewModel: ViewModel) {
        deviceDescriptionLabel.text = viewModel.deviceDescription
    }

    func showActivity(active: Bool) {
        if active {
            actionView.fView.isEnabled = false
            actionView.sView.startLoading()
        } else {
            actionView.sView.stopLoading()
            actionView.fView.isEnabled = true
        }
    }
}

// MARK: - AccessibilityBound

extension PolkadotSignInResultView: AccessibilityBound {
    public var accessibilityBindings: [AccessibilityBinding] {
        [
            .init(titleLabel, AccessibilityID.Pairing.title),
            .init(cancelButton, AccessibilityID.Pairing.rejectButton),
            .init(linkButton, AccessibilityID.Pairing.confirmButton)
        ]
    }
}
