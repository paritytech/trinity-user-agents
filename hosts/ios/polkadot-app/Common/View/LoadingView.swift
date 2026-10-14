import UIKit
import UIKit_iOS
import PolkadotUI

final class LoaderView: UIView {
    private let loaderView: LoadingView = .create { view in
        view.contentBackgroundColor = .clear
        view.contentSize = CGSize(width: 40, height: 40)
        view.indicatorImage = .loadingIndicator.withRenderingMode(.alwaysTemplate)
    }

    private let stackView: UIStackView = .create { view in
        view.axis = .vertical
        view.spacing = 12
        view.alignment = .fill
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupView()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func startAnimating() {
        guard !loaderView.isAnimating else {
            return
        }
        loaderView.startAnimating()
    }

    func stopAnimating() {
        loaderView.stopAnimating()
    }

    override var tintColor: UIColor! {
        didSet {
            loaderView.tintColor = tintColor
        }
    }
}

extension LoaderView {
    private func setupView() {
        addSubview(stackView)
        stackView.addArrangedSubviews([loaderView])

        stackView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        loaderView.snp.makeConstraints { make in
            make.height.equalTo(48)
        }
    }
}
