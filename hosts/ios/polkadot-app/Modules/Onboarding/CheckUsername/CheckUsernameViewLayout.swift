import UIKit
import SnapKit
import PolkadotUI
import DesignSystem

final class CheckUsernameViewLayout: UIView {
    private let activityIndicatorView = ActivityIndicatorView()

    private let messageLabel: Label = .create { view in
        view.numberOfLines = 0
        view.textAlignment = .center
        view.typography = .titleLarge
        view.textColor = .fgPrimary
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - ViewModel

extension CheckUsernameViewLayout {
    enum ViewModel {
        case loading
        case error
    }

    func bind(viewModel: ViewModel) {
        switch viewModel {
        case .loading:
            messageLabel.isHidden = true
            activityIndicatorView.text = String(localized: .recoveringAccountDescription)
            activityIndicatorView.startAnimating()
        case .error:
            messageLabel.isHidden = false
            activityIndicatorView.stopAnimating()
            messageLabel.text = String(localized: .claimUsernameSetupError)
        }
    }
}

// MARK: - Private

private extension CheckUsernameViewLayout {
    func setupLayout() {
        backgroundColor = .bgSurfaceMain

        addSubview(activityIndicatorView)
        activityIndicatorView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(32)
            $0.centerY.equalToSuperview()
        }

        addSubview(messageLabel)
        messageLabel.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(32)
            make.centerY.equalToSuperview()
        }
    }
}
