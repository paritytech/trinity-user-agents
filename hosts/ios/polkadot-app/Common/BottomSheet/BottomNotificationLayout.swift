import UIKit
import PolkadotUI
import DesignSystem

enum BottomNotificationConstants {
    static let contentInsets = UIEdgeInsets(top: 0, left: 16, bottom: 24, right: 16)
}

final class BottomNotificationLayout: UIView {
    enum Constants {
        static let backgroundInsets = UIEdgeInsets(top: 16, left: 24, bottom: 16, right: 24)
    }

    let backgroundView: GenericBackgroundView<Label> = .create { view in
        view.applyBackgroundStyle(.bgSurfaceContainer, cornerRadius: 26)
        view.wrappedView.typography = .titleMedium
        view.wrappedView.textColor = .fgPrimary
        view.wrappedView.numberOfLines = 0
        view.insets = Constants.backgroundInsets
    }

    var titleLabel: UILabel {
        backgroundView.wrappedView
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupLayout() {
        addSubview(backgroundView)

        backgroundView.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(BottomNotificationConstants.contentInsets.left)
            make.trailing.equalToSuperview().inset(BottomNotificationConstants.contentInsets.right)
            make.bottom.equalToSuperview().inset(BottomNotificationConstants.contentInsets.bottom)
        }
    }
}

extension BottomNotificationLayout {
    static func estimateContentHeight(for text: String) -> CGFloat {
        let size = UIScreen.main.bounds.size

        let availableWidth = min(size.width, size.height) -
            BottomNotificationConstants.contentInsets.left -
            BottomNotificationConstants.contentInsets.right -
            BottomNotificationLayout.Constants.backgroundInsets.left -
            BottomNotificationLayout.Constants.backgroundInsets.right

        let height = text.estimateHeight(for: UIFont.titleMedium, width: availableWidth)

        return height + BottomNotificationLayout.Constants.backgroundInsets.top +
            BottomNotificationLayout.Constants.backgroundInsets.bottom
    }
}
