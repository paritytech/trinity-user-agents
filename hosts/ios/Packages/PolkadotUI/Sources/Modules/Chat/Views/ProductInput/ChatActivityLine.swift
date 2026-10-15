import DesignSystem
import UIKit
internal import SnapKit

/// One line above the input that tells what happens now, for example "Bot is typing", with a typing animation.
final class ChatActivityLine: UIView {
    private let dots = UIImageView(image: UIImage(systemName: "ellipsis"))
    private let label: Label = create {
        $0.typography = .captionMedium
        $0.textColor = .fgSecondary
        $0.numberOfLines = 1
        $0.lineBreakMode = .byTruncatingTail
    }

    /// The dots move, which they do not when the user asks for reduced motion (P7.2).
    private(set) var animatesDots = false

    var text: String? {
        didSet {
            label.text = text
            isHidden = text == nil
            accessibilityLabel = text
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        isAccessibilityElement = true
        dots.tintColor = .fgSecondary
        dots.contentMode = .scaleAspectFit
        updateDotsAnimation()
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(reduceMotionChanged),
            name: UIAccessibility.reduceMotionStatusDidChangeNotification,
            object: nil
        )

        addSubview(dots)
        addSubview(label)
        dots.snp.makeConstraints {
            $0.leading.equalToSuperview().inset(DSSpacings.medium)
            $0.centerY.equalTo(label)
            $0.width.equalTo(20)
        }
        label.snp.makeConstraints {
            $0.leading.equalTo(dots.snp.trailing).offset(DSSpacings.extraSmall)
            $0.trailing.equalToSuperview().inset(DSSpacings.medium)
            $0.top.bottom.equalToSuperview().inset(DSSpacings.extraTiny)
        }
        isHidden = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension ChatActivityLine {
    func updateDotsAnimation() {
        dots.removeAllSymbolEffects()
        animatesDots = !UIAccessibility.isReduceMotionEnabled
        if animatesDots {
            dots.addSymbolEffect(.variableColor.iterative.reversing)
        }
    }

    @objc func reduceMotionChanged() {
        updateDotsAnimation()
    }
}
