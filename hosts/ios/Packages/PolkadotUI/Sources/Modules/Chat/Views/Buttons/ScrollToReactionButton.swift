import DesignSystem
import UIKit

internal import SnapKit

final class ScrollToReactionButton: UIControl {
    private let iconButton: DSIconButton = .chatScrollToReaction

    override var intrinsicContentSize: CGSize {
        iconButton.intrinsicContentSize
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }
}

// MARK: Private functions

extension ScrollToReactionButton {
    private func setup() {
        iconButton.onTap = { [weak self] in
            self?.sendActions(for: .touchUpInside)
        }

        addSubview(iconButton)
        iconButton.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }
}
