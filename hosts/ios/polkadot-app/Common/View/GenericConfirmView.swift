import UIKit
import UIKit_iOS
import PolkadotUI
import DesignSystem

final class GenericConfirmView<View: UIControl>: UIView {
    let issueView: GenericBorderedView<Label> = .create { view in
        view.backgroundView.applyBackgroundStyle(
            .fill6,
            cornerRadius: 12
        )

        view.contentView.typography = .titleMedium
        view.contentView.textColor = .fgDisabled

        view.contentView.textAlignment = .center
    }

    let errorButton: RoundedButton = .create { button in
        button.apply(style: .transparent)
        button.imageWithTitleView?.spacingBetweenLabelAndIcon = 8
    }

    let actionButton = View()

    let loadingView: LoaderView = .create { view in
        view.backgroundColor = .white
        view.tintColor = .black100

        if #available(iOS 26.0, *) {
            view.cornerConfiguration = .corners(radius: 12)
        } else {
            view.layer.cornerRadius = 12
        }
    }

    var isLoading: Bool {
        loadingView.superview != nil
    }

    override var intrinsicContentSize: CGSize {
        CGSize(width: UIView.noIntrinsicMetric, height: UIConstants.actionHeight)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        bind(state: .loading)
        loadingView.snp.makeConstraints { make in
            make.height.equalTo(52)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func bind(state: GenericConfirmView<some UIControl>.State) {
        switch state {
        case .loading:
            set(subview: errorButton, visible: false)
            set(subview: actionButton, visible: false)
            set(subview: issueView, visible: false)
            set(subview: loadingView, visible: true)

            loadingView.startAnimating()

        case let .issue(title):
            loadingView.stopAnimating()

            set(subview: errorButton, visible: false)
            set(subview: actionButton, visible: false)
            set(subview: issueView, visible: true)
            set(subview: loadingView, visible: false)

            issueView.contentView.text = title

        case let .errorAction(title, icon):
            loadingView.stopAnimating()

            set(subview: errorButton, visible: true)
            set(subview: actionButton, visible: false)
            set(subview: issueView, visible: false)
            set(subview: loadingView, visible: false)

            errorButton.setTitle(title)
            errorButton.setIcon(icon)

        case .confirm:
            loadingView.stopAnimating()
            set(subview: errorButton, visible: false)
            set(subview: actionButton, visible: true)
            set(subview: issueView, visible: false)
            set(subview: loadingView, visible: false)
        }

        setNeedsLayout()
    }

    private func set(subview: UIView, visible: Bool) {
        guard visible else {
            subview.removeFromSuperview()
            return
        }

        guard subview.superview == nil else {
            return
        }
        addSubview(subview)
        subview.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }
}

extension GenericConfirmView {
    enum State {
        case loading
        case issue(String)
        case errorAction(String, UIImage?)
        case confirm
    }
}
