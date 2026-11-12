import UIKit
import SnapKit
import ExternalAccessibility
import PolkadotUI

final class SelfTransferTableViewCell: PlainBaseTableViewCell<SelfTransferContentView> {
    override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
        super.init(style: style, reuseIdentifier: reuseIdentifier)
    }

    // MARK: Public methods

    func bind(isLoading: Bool) {
        contentDisplayView.bind(isLoading: isLoading)
    }
}

final class SelfTransferContentView: UIView {
    // MARK: Properties

    private let iconCircle: UIView = .create {
        $0.backgroundColor = .bgSurfaceContainer
        $0.layer.cornerRadius = 20
        $0.clipsToBounds = true
    }

    private let iconView: UIImageView = .create {
        $0.image = UIImage(resource: .iconWithdraw).withRenderingMode(.alwaysTemplate)
        $0.tintColor = .fgPrimary
    }

    private let titleLabel: Label = .create {
        $0.typography = .titleMedium
        $0.textColor = .fgPrimary
        $0.text = String(localized: .searchAccountSelfTransferTitle)
    }

    private let subtitleLabel: Label = .create {
        $0.typography = .bodyMedium
        $0.textColor = .fgSecondary
        $0.text = String(localized: .searchAccountSelfTransferSubtitle)
    }

    private let labelsStack: UIStackView = .create {
        $0.axis = .vertical
    }

    private let loadingIndicator: UIActivityIndicatorView = .create {
        $0.hidesWhenStopped = true
    }

    // MARK: Initial methods

    override init(frame: CGRect) {
        super.init(frame: frame)

        configureConstraints()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Public methods

    fileprivate func bind(isLoading: Bool) {
        if isLoading {
            loadingIndicator.startAnimating()
        } else {
            loadingIndicator.stopAnimating()
        }

        isUserInteractionEnabled = !isLoading
    }

    // MARK: Private methods

    private func configureConstraints() {
        iconCircle.addSubview(iconView)
        labelsStack.addArrangedSubview(titleLabel)
        labelsStack.addArrangedSubview(subtitleLabel)
        addSubview(iconCircle)
        addSubview(labelsStack)
        addSubview(loadingIndicator)

        iconCircle.snp.makeConstraints {
            $0.centerY.equalToSuperview()
            $0.left.equalToSuperview().inset(16)
            $0.top.bottom.equalToSuperview().inset(8).priority(.high)
            $0.height.width.equalTo(40)
        }

        iconView.snp.makeConstraints {
            $0.center.equalToSuperview()
            $0.height.width.equalTo(24)
        }

        labelsStack.snp.makeConstraints {
            $0.leading.equalTo(iconCircle.snp.trailing).offset(12)
            $0.centerY.equalToSuperview()
            $0.trailing.lessThanOrEqualTo(loadingIndicator.snp.leading).offset(-8)
        }

        loadingIndicator.snp.makeConstraints {
            $0.centerY.equalToSuperview()
            $0.trailing.equalToSuperview().inset(16)
        }
    }
}

// MARK: - AccessibilityBound

extension SelfTransferContentView: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [.init(self, AccessibilityID.Wallet.withdrawButton)]
    }
}
