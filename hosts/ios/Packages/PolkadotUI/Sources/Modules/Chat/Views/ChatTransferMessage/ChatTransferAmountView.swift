import UIKit
import DesignSystem

final class ChatTransferAmountView: UIView {
    let amountLabel: UILabel = create {
        $0.numberOfLines = 1
    }

    let unitLabel: UILabel = create {
        $0.numberOfLines = 1
    }

    private(set) var typography: TypographyStyle = Constants.typographyLadder[0]

    override init(frame: CGRect) {
        super.init(frame: frame)

        addSubview(amountLabel)
        addSubview(unitLabel)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var intrinsicContentSize: CGSize {
        let amountSize = amountLabel.intrinsicContentSize
        let unitSize = unitLabel.intrinsicContentSize

        return CGSize(
            width: amountSize.width + unitSize.width,
            height: max(amountSize.height, unitSize.height)
        )
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        let amountSize = amountLabel.intrinsicContentSize
        let unitSize = unitLabel.intrinsicContentSize
        let amountFont = amountLabel.font ?? UIFont.app(typography)
        let unitFont = unitLabel.font ?? amountFont

        amountLabel.frame = CGRect(origin: .zero, size: amountSize)
        unitLabel.frame = CGRect(
            x: amountSize.width,
            y: amountFont.ascender - unitFont.ascender,
            width: unitSize.width,
            height: unitSize.height
        )
    }

    func bind(currencySymbol: String, amount: String, unit: String) {
        let amountText = currencySymbol + amount

        typography = Constants.typographyLadder.first { fits(amountText: amountText, unit: unit, typography: $0) }
            ?? Constants.typographyLadder[Constants.typographyLadder.count - 1]

        let font = UIFont.app(typography)
        amountLabel.font = font
        amountLabel.text = amountText
        unitLabel.font = unitFont(for: font)
        unitLabel.text = unit

        invalidateIntrinsicContentSize()
        setNeedsLayout()
    }
}

private extension ChatTransferAmountView {
    enum Constants {
        static let typographyLadder: [TypographyStyle] = [.headlineLarge, .headlineSmall, .titleExtraLarge]
        static let maxWidth: CGFloat = 160
    }

    func unitFont(for font: UIFont) -> UIFont {
        UIFont.app(.smallCapsHeadlineMedium).withSize(font.pointSize)
    }

    func fits(amountText: String, unit: String, typography: TypographyStyle) -> Bool {
        let font = UIFont.app(typography)
        let total = width(of: amountText, font: font) + width(of: unit, font: unitFont(for: font))

        return total <= Constants.maxWidth
    }

    func width(of text: String, font: UIFont) -> CGFloat {
        (text as NSString).size(withAttributes: [.font: font]).width
    }
}
