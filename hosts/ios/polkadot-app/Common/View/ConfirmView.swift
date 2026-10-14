import UIKit
import UIKit_iOS
import PolkadotUI
import DesignSystem

class ConfirmView: UIView {
    let issueView: GenericBackgroundView<Label> = .create { view in
        view.applyBackgroundStyle(
            .fill6,
            cornerRadius: UIConstants.actionHeight / 2
        )

        view.wrappedView.typography = .titleMedium
        view.wrappedView.textColor = .fgDisabled

        view.wrappedView.textAlignment = .center
    }

    let errorButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.spacingBetweenLabelAndIcon = 8
    }

    let actionView: LoadableActionView = .create { view in
        view.applyMainStyle()
    }

    var actionButton: RoundedButton {
        actionView.actionButton
    }

    var isLoading: Bool {
        !actionView.isHidden && actionView.actionLoadingView.activityIndicator.isAnimating
    }

    override var intrinsicContentSize: CGSize {
        CGSize(width: UIView.noIntrinsicMetric, height: UIConstants.actionHeight)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
        bind(state: .loading)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func bind(state: ConfirmView.State) {
        switch state {
        case .loading:
            errorButton.isHidden = true
            actionView.isHidden = false
            issueView.isHidden = true

            actionView.startLoading()

        case let .issue(title):
            actionView.stopLoading()

            errorButton.isHidden = true
            actionView.isHidden = true
            issueView.isHidden = false

            issueView.wrappedView.text = title.uppercased()

        case let .errorAction(title, icon):
            actionView.stopLoading()

            errorButton.isHidden = false
            actionButton.isHidden = true
            issueView.isHidden = true

            errorButton.setTitle(title)
            errorButton.setIcon(icon)

        case .confirm:
            actionView.stopLoading()

            errorButton.isHidden = true
            actionView.isHidden = false
            issueView.isHidden = true
            actionButton.isEnabled = true

        case .disabled:
            actionView.stopLoading()

            errorButton.isHidden = true
            actionView.isHidden = false
            issueView.isHidden = true
            actionButton.isEnabled = false
        }

        setNeedsLayout()
    }

    private func setupLayout() {
        addSubview(issueView)
        issueView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addSubview(errorButton)
        errorButton.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addSubview(actionView)
        actionView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }
}

extension ConfirmView {
    enum State {
        case loading
        case issue(String)
        case errorAction(String, UIImage?)
        case confirm
        case disabled
    }
}
