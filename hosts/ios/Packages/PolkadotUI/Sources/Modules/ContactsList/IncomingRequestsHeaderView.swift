import DesignSystem
import ExternalAccessibility
import UIKit
internal import UIKit_iOS
internal import SnapKit

public struct IncomingRequestsHeaderConfiguration: HashableContentConfiguration {
    let requestCount: Int

    public func hash(into hasher: inout Hasher) {
        hasher.combine(requestCount)
    }

    public static func == (lhs: IncomingRequestsHeaderConfiguration, rhs: IncomingRequestsHeaderConfiguration) -> Bool {
        lhs.requestCount == rhs.requestCount
    }

    public init(requestCount: Int) {
        self.requestCount = requestCount
    }

    public func makeContentView() -> any UIView & UIContentView {
        IncomingRequestsHeaderView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }
}

final class IncomingRequestsHeaderView: UIView, UIContentView {
    private static let height: CGFloat = 48
    private static let verticalInset: CGFloat = 8

    private let backgroundView: RoundedView = .create {
        $0.applyBackgroundStyle(
            UIColor.bgSurfaceContainer,
            cornerRadius: 24
        )
    }

    private let iconImageView: UIImageView = .create {
        $0.contentMode = .scaleAspectFit
        $0.tintColor = UIColor.fgPrimary
        $0.image = UIImage(resource: .markChatUnread).withRenderingMode(.alwaysTemplate)
    }

    private let titleLabel: Label = .create {
        $0.typography = .paragraphLarge
        $0.textColor = UIColor.fgPrimary
        $0.numberOfLines = 1
    }

    private let countLabel: GenericBorderedView<Label> = .create {
        $0.backgroundView.applyBackgroundStyle(
            UIColor.fgPrimary,
            cornerRadius: 11
        )
        $0.contentInsets = UIEdgeInsets(top: 1, left: 8, bottom: 2, right: 8)
        $0.contentView.typography = .titleSmall
        $0.contentView.textColor = UIColor.fgPrimaryInverted
    }

    private let arrowImageView: UIImageView = .create {
        $0.contentMode = .scaleAspectFit
        $0.tintColor = UIColor.fgSecondary
        $0.image = UIImage(resource: .arrowForward).withRenderingMode(.alwaysTemplate)
    }

    private var appliedConfiguration: IncomingRequestsHeaderConfiguration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: IncomingRequestsHeaderConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        addSubview(backgroundView)
        backgroundView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(DSSpacings.extraMedium)
            $0.top.bottom.equalToSuperview().inset(Self.verticalInset)
            $0.height.equalTo(Self.height)
        }

        backgroundView.addSubview(iconImageView)
        iconImageView.snp.makeConstraints {
            $0.leading.equalToSuperview().inset(20)
            $0.centerY.equalToSuperview()
        }

        backgroundView.addSubview(titleLabel)
        titleLabel.snp.makeConstraints {
            $0.leading.equalTo(iconImageView.snp.trailing).offset(16)
            $0.centerY.equalToSuperview()
        }

        backgroundView.addSubview(arrowImageView)
        arrowImageView.snp.makeConstraints {
            $0.trailing.equalToSuperview().inset(16)
            $0.centerY.equalToSuperview()
        }

        backgroundView.addSubview(countLabel)
        countLabel.snp.makeConstraints {
            $0.trailing.equalTo(arrowImageView.snp.leading).offset(-8)
            $0.centerY.equalToSuperview()
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? IncomingRequestsHeaderConfiguration else { return }
        appliedConfiguration = configuration

        titleLabel.text = String(localized: .commonNewRequests)

        if configuration.requestCount > 0 {
            countLabel.setHidden(false)
            countLabel.contentView.text = String(configuration.requestCount)
        } else {
            countLabel.setHidden(true)
            countLabel.contentView.text = ""
        }
    }
}

// MARK: - AccessibilityBound

extension IncomingRequestsHeaderView: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [.init(self, AccessibilityID.Chats.newRequestsItem)]
    }
}
