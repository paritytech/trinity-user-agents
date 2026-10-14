import UIKit
import SnapKit
import PolkadotUI
import DesignSystem

final class BackupInfoView: GenericPairValueView<GenericBorderedView<UIImageView>, TopBottomLabelView> {
    // MARK: Properties

    struct ImageConfiguration {
        let image: UIImage?
        let backgroundColor: UIColor
    }

    struct LabelConfiguration {
        var text: String?
        let textAlignment: NSTextAlignment
        let font: UIFont
        var textColor: UIColor
        let numberOfLines: Int

        init(
            text: String? = nil,
            textAlignment: NSTextAlignment,
            font: UIFont,
            textColor: UIColor,
            numberOfLines: Int = 1
        ) {
            self.text = text
            self.textAlignment = textAlignment
            self.font = font
            self.textColor = textColor
            self.numberOfLines = numberOfLines
        }
    }

    fileprivate protocol InfoTypeProtocol {
        var imageConfiguration: ImageConfiguration { get }
        var titleConfiguration: LabelConfiguration { get }
        var subtitleConfiguration: LabelConfiguration { get }
    }

    private var imageView: UIImageView {
        let imageView = fView.contentView
        imageView.contentMode = .scaleAspectFit
        imageView.tintColor = .fgPrimary
        return imageView
    }

    private var titleLabel: UILabel {
        sView.topLabel
    }

    private var subtitleLabel: UILabel {
        sView.bottomLabel
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        configureView()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Public methods

    func bind(type: InfoType) {
        configureImage(for: type.imageConfiguration)
        configureLabel(titleLabel, with: type.titleConfiguration)
        configureLabel(subtitleLabel, with: type.subtitleConfiguration)
    }

    // MARK: Private methods

    private func configureImage(for configurator: ImageConfiguration) {
        imageView.image = configurator.image
        fView.backgroundView.applyBackgroundStyle(
            configurator.backgroundColor,
            cornerRadius: Constants.backgroundIconSize / 2
        )
    }

    private func configureLabel(_ label: UILabel, with configurator: LabelConfiguration) {
        label.text = configurator.text
        label.textColor = configurator.textColor
        label.font = configurator.font
        label.textAlignment = configurator.textAlignment
        label.numberOfLines = configurator.numberOfLines
    }

    private func configureView() {
        stackView.alignment = .center
        stackView.setCustomSpacing(Constants.defaultOffset, after: fView)
        sView.spacing = Constants.spacingBetweenLabels

        sView.snp.makeConstraints {
            $0.left.right.equalToSuperview().inset(Constants.defaultOffset)
        }

        fView.backgroundView.snp.makeConstraints {
            $0.height.width.equalTo(Constants.backgroundIconSize)
        }
        imageView.snp.remakeConstraints {
            $0.height.width.equalTo(Constants.iconSize)
            $0.center.equalToSuperview()
        }
    }
}

// MARK: - Constants

private enum Constants {
    static let backgroundIconSize: CGFloat = 104
    static let spacingBetweenLabels: CGFloat = 8
    static let defaultOffset: CGFloat = 24
    static let iconSize: CGFloat = 54
}

extension BackupInfoView {
    enum InfoType: InfoTypeProtocol {
        case backup(Backup)
    }
}

extension BackupInfoView.InfoType {
    enum Backup: BackupInfoView.InfoTypeProtocol {
        case notFound
        case created
        case icloudIsOff

        private var defaultTitleConfiguration: BackupInfoView.LabelConfiguration {
            BackupInfoView.LabelConfiguration(
                textAlignment: .center,
                font: UIFont.headlineSmall,
                textColor: .fgPrimary
            )
        }

        private var defaultSubtitleConfiguration: BackupInfoView.LabelConfiguration {
            BackupInfoView.LabelConfiguration(
                textAlignment: .center,
                font: UIFont.paragraphLarge,
                textColor: .fgPrimary,
                numberOfLines: .zero
            )
        }

        var imageConfiguration: BackupInfoView.ImageConfiguration {
            switch self {
            case .created:
                BackupInfoView.ImageConfiguration(image: .iconCloudBig, backgroundColor: .bgSurfaceContainer)
            case .notFound:
                BackupInfoView.ImageConfiguration(image: .iconCloudOff, backgroundColor: .bgSurfaceContainer)
            case .icloudIsOff:
                BackupInfoView.ImageConfiguration(image: .iconCloudOff, backgroundColor: .bgSurfaceContainer)
            }
        }

        var titleConfiguration: BackupInfoView.LabelConfiguration {
            var config = defaultTitleConfiguration
            switch self {
            case .created:
                config.text = String(localized: .backupInfoCreatedTitle)
            case .notFound:
                config.text = String(localized: .backupInfoNotfoundTitle)
            case .icloudIsOff:
                config.text = String(localized: .backupInfoDisabledTitle)
            }
            return config
        }

        var subtitleConfiguration: BackupInfoView.LabelConfiguration {
            var config = defaultSubtitleConfiguration
            switch self {
            case .created:
                config.text = String(localized: .backupInfoCreatedSubtitle)
            case .notFound:
                config.text = String(localized: .backupInfoNotfoundSubtitle)
            case .icloudIsOff:
                config.text = String(localized: .backupInfoDisabledSubtitle)
            }
            return config
        }
    }
}

extension BackupInfoView.InfoType {
    var imageConfiguration: BackupInfoView.ImageConfiguration {
        switch self {
        case let .backup(type):
            type.imageConfiguration
        }
    }

    var titleConfiguration: BackupInfoView.LabelConfiguration {
        switch self {
        case let .backup(type):
            type.titleConfiguration
        }
    }

    var subtitleConfiguration: BackupInfoView.LabelConfiguration {
        switch self {
        case let .backup(type):
            type.subtitleConfiguration
        }
    }
}
