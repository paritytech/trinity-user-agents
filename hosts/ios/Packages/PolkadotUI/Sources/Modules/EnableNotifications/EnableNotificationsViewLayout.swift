import UIKit
import DesignSystem
public import UIKit_iOS
internal import SnapKit

public final class EnableNotificationsViewLayout: UIView {
    public let contentView: UIView = create {
        $0.setHidden(true)
    }

    public let titleLabel: Label = .create {
        $0.typography = .headlineLarge
        $0.textColor = UIColor(resource: .textAndIconsPrimaryDark)
        $0.numberOfLines = 0
        $0.text = String(localized: .Notification.enableNotificationTitle)
    }

    public let reasonsScrollableView: ScrollableContainerLayoutView = .create {
        $0.backgroundColor = .clear
        $0.stackView.spacing = 8
        $0.layoutInsets.left = 24
        $0.layoutInsets.right = 24
    }

    public let enableButton: RoundedButton = .create { button in
        button.applyMainStyle()
    }

    public let ignoreButton: RoundedButton = .create { button in
        button.applyTitleTertiaryStyle()
        button.setTitle(String(localized: .Notification.enableNotificationIgnoreButton))
    }

    public let additionalInfoViewContainer: UIStackView = create {
        $0.isLayoutMarginsRelativeArrangement = true
        $0.layoutMargins.top = 20
    }

    let additionalInfoView: GenericBackgroundView<GenericPairValueView<Label, UIImageView>> = create {
        $0.insets = UIEdgeInsets(top: 16, left: 16, bottom: 16, right: 16)
        $0.applyBorderStyle(UIColor(resource: .white8), cornerRadius: 16)

        $0.wrappedView.spacing = 24
        $0.wrappedView.stackView.axis = .horizontal
        $0.wrappedView.stackView.alignment = .center

        $0.wrappedView.fView.numberOfLines = 0
        $0.wrappedView.fView.typography = .bodyMedium
        $0.wrappedView.fView.textColor = UIColor(resource: .textAndIconsPrimaryDark)

        $0.wrappedView.sView.snp.makeConstraints {
            $0.width.height.equalTo(30)
        }
    }

    public var additionalInfoLabel: UILabel {
        additionalInfoView.wrappedView.fView
    }

    public var additionalInfoImageView: UIImageView {
        additionalInfoView.wrappedView.sView
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        backgroundColor = UIColor(resource: .backgroundPrimary)

        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension EnableNotificationsViewLayout {
    func setupLayout() {
        additionalInfoViewContainer.addArrangedSubview(additionalInfoView)

        addSubview(contentView)
        contentView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        contentView.addSubview(titleLabel)
        contentView.addSubview(reasonsScrollableView)
        contentView.addSubview(additionalInfoViewContainer)
        contentView.addSubview(enableButton)
        contentView.addSubview(ignoreButton)

        titleLabel.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(24)
            $0.top.equalTo(safeAreaLayoutGuide.snp.top).offset(52)
        }

        let scrollView = reasonsScrollableView.containerView.scrollView
        reasonsScrollableView.snp.makeConstraints {
            // make scrollView autosize
            $0.height.equalTo(scrollView.contentLayoutGuide.snp.height).priority(.low)
            $0.top.equalTo(titleLabel.snp.bottom).offset(40)
            $0.leading.trailing.equalToSuperview()
            $0.bottom.lessThanOrEqualTo(additionalInfoViewContainer.snp.top)
        }

        additionalInfoViewContainer.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(24)
            $0.bottom.equalTo(enableButton.snp.top).offset(-24)
        }

        enableButton.snp.makeConstraints {
            $0.height.equalTo(UIConstants.actionHeight)
            $0.leading.trailing.equalToSuperview().inset(24)
            $0.bottom.equalTo(ignoreButton.snp.top).offset(-16)
        }

        ignoreButton.snp.makeConstraints {
            $0.height.equalTo(UIConstants.actionHeight)
            $0.leading.trailing.equalToSuperview().inset(24)
            $0.bottom.equalTo(safeAreaLayoutGuide.snp.bottom).inset(16)
        }
    }

    func configureReasonsStack(viewModel: ViewModel) {
        // clean up
        reasonsScrollableView.stackView.arrangedSubviews.forEach {
            reasonsScrollableView.stackView.removeArrangedSubview($0)
            $0.removeFromSuperview()
        }

        let rows = viewModel.reasons.map {
            ReasonToEnableNotificationRow(viewModel: $0)
        }

        rows.forEach { row in
            reasonsScrollableView.addArrangedSubview(row)
        }
    }

    func configureAdditionalInfo(viewModel: ViewModel) {
        if let additionalInfo = viewModel.additionalInfo {
            additionalInfoLabel.attributedText = additionalInfo.info
            additionalInfoImageView.image = additionalInfo.icon

            additionalInfoViewContainer.isLayoutMarginsRelativeArrangement = true
            additionalInfoView.setHidden(false)
        } else {
            additionalInfoViewContainer.isLayoutMarginsRelativeArrangement = false
            additionalInfoView.setHidden(true)
        }
    }
}

public extension EnableNotificationsViewLayout {
    struct ViewModel {
        public struct AdditionalInfoModel {
            let info: NSAttributedString
            let icon: UIImage

            public init(info: NSAttributedString, icon: UIImage) {
                self.info = info
                self.icon = icon
            }
        }

        let reasons: [ReasonToEnableNotificationRow.ViewModel]
        let additionalInfo: AdditionalInfoModel?
        let enableTitle: String

        public init(
            reasons: [ReasonToEnableNotificationRow.ViewModel],
            additionalInfo: AdditionalInfoModel?,
            enableTitle: String
        ) {
            self.reasons = reasons
            self.additionalInfo = additionalInfo
            self.enableTitle = enableTitle
        }
    }

    func bind(viewModel: ViewModel) {
        contentView.setHidden(false)
        configureReasonsStack(viewModel: viewModel)
        configureAdditionalInfo(viewModel: viewModel)
        enableButton.setTitle(viewModel.enableTitle)
    }
}
