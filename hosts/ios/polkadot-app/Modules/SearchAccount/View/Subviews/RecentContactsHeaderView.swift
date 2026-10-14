import UIKit
import SnapKit
import PolkadotUI

final class RecentContactsHeaderView: UIView {
    // MARK: Properties

    private let titleLabel: Label = .create {
        $0.style = .title16SemiBold()
        $0.textColor = .fgPrimary
    }

    private let title: String?

    // MARK: Initial methods

    required init(with title: String?, frame: CGRect = .zero) {
        self.title = title
        super.init(frame: frame)

        configureView()
    }

    @available(*, unavailable)
    override private init(frame _: CGRect) {
        fatalError("init(frame:) has not been implemented")
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Private methods

    private func configureView() {
        backgroundColor = .bgSurfaceMain
        titleLabel.text = title
        addSubview(titleLabel)
        titleLabel.snp.makeConstraints {
            $0.top.bottom.equalToSuperview()
            $0.leading.trailing.equalToSuperview().inset(24)
        }
    }
}
