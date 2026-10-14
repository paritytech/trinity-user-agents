import DesignSystem
import PolkadotUI
import SnapKit
import UIKit

final class BrowseViewLayout: UIView {
    let activityIndicatorView = ActivityIndicatorView()

    override init(frame: CGRect) {
        super.init(frame: frame)

        backgroundColor = .bgSurfaceMain

        setupActivityIndicatorLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - Private

private extension BrowseViewLayout {
    func setupActivityIndicatorLayout() {
        addSubview(activityIndicatorView)
        activityIndicatorView.snp.makeConstraints { make in
            make.center.equalToSuperview()
        }
    }
}
