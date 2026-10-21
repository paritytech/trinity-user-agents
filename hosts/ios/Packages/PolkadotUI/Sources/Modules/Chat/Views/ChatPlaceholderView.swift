import DesignSystem
import UIKit
internal import SnapKit

/// The text in the middle of a chat that has no messages, for example "No messages yet".
public final class ChatPlaceholderView: UIView {
    private enum Constants {
        static let horizontalInset: CGFloat = 32
    }

    private let label: Label = create {
        $0.typography = .paragraphMedium
        $0.textColor = .fgSecondary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    /// Nil hides the view.
    public var text: String? {
        didSet {
            label.text = text
            isHidden = text == nil
        }
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        isHidden = true
        isUserInteractionEnabled = false

        addSubview(label)
        label.snp.makeConstraints {
            $0.center.equalToSuperview()
            $0.leading.trailing.equalToSuperview().inset(Constants.horizontalInset)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
