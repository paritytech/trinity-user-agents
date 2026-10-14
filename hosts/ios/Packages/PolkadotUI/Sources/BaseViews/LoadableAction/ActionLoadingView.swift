import UIKit
public import UIKit_iOS

public final class ActionLoadingView: UIView {
    public let backgroundView: RoundedView = {
        let view = RoundedView()
        view.applyPrimaryMedium()
        view.cornerRadius = 12.0
        return view
    }()

    public let activityIndicator: UIActivityIndicatorView = {
        let view = UIActivityIndicatorView()
        view.style = .medium
        view.color = UIColor(resource: .white100)
        view.hidesWhenStopped = true
        return view
    }()

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func start() {
        activityIndicator.startAnimating()
    }

    public func stop() {
        activityIndicator.stopAnimating()
    }

    public var isAnimating: Bool {
        activityIndicator.isAnimating
    }

    private func setupLayout() {
        addSubview(backgroundView)
        backgroundView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addSubview(activityIndicator)
        activityIndicator.snp.makeConstraints { make in
            make.center.equalToSuperview()
        }
    }
}
