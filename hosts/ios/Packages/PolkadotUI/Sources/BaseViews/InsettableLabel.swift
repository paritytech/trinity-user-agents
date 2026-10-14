import UIKit

public final class InsettableLabel: Label {
    public var insets: UIEdgeInsets

    override public init(frame: CGRect) {
        insets = .zero
        super.init(frame: frame)
    }

    public init(_ insets: UIEdgeInsets = .zero) {
        self.insets = insets
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func drawText(in rect: CGRect) {
        super.drawText(in: rect.inset(by: insets))
    }

    override public var intrinsicContentSize: CGSize {
        var size = super.intrinsicContentSize
        size.width += insets.left + insets.right
        size.height += insets.top + insets.bottom
        return size
    }
}
