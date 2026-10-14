import UIKit
import DesignSystem
internal import UIKit_iOS

public final class ActivityIndicatorView: UIView {
    private let textLabel: Label = create {
        $0.numberOfLines = 0
        $0.textAlignment = .center
        $0.typography = .titleLarge
        $0.textColor = .fgPrimary
    }

    let loadingView: LoadingView = create {
        $0.contentBackgroundColor = .clear
        $0.contentSize = .init(width: Constants.loadingViewSize, height: Constants.loadingViewSize)
        $0.indicatorImage = UIImage(resource: .loading).withRenderingMode(.alwaysTemplate)
        $0.tintColor = .fgPrimary
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - Internal

public extension ActivityIndicatorView {
    var text: String? {
        get { textLabel.text }
        set { textLabel.text = newValue }
    }

    func startAnimating(after delay: TimeInterval = 0) {
        if delay > 0 {
            perform(
                #selector(performStartAnimating),
                with: nil,
                afterDelay: delay
            )
        } else {
            performStartAnimating()
        }
    }

    func stopAnimating() {
        NSObject.cancelPreviousPerformRequests(withTarget: self)
        performStopAnimating()
    }
}

// MARK: - Private

private extension ActivityIndicatorView {
    enum Constants {
        static let loadingViewSize = CGFloat(64)
    }

    func setupLayout() {
        alpha = 0
        backgroundColor = .clear

        addSubview(textLabel)
        textLabel.snp.makeConstraints {
            $0.leading.trailing.top.equalToSuperview()
            $0.width.greaterThanOrEqualTo(Constants.loadingViewSize)
        }

        addSubview(loadingView)
        loadingView.snp.makeConstraints {
            $0.top.equalTo(textLabel.snp.bottom).inset(-64)
            $0.bottom.centerX.equalToSuperview()
        }
    }

    @objc
    func performStartAnimating() {
        UIView.animate(withDuration: 0.25) { [self] in
            alpha = 1
        }
        loadingView.startAnimating()
    }

    func performStopAnimating() {
        UIView.animate(withDuration: 0.25) { [self] in
            alpha = 0
        }
        loadingView.stopAnimating()
    }
}
