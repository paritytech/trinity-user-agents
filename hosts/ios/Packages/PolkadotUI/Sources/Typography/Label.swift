import UIKit
import DesignSystem

open class Label: UILabel {
    public var style: LabelStyle? {
        didSet {
            updateText()
        }
    }

    public var typography: TypographyStyle? {
        didSet {
            applyTypography()
        }
    }

    private var typographyRegistration: UITraitChangeRegistration?

    public convenience init(text: String?, textColor: UIColor) {
        self.init()
        self.text = text
        self.textColor = textColor
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        commonInit()
    }

    public required init?(coder aDecoder: NSCoder) {
        super.init(coder: aDecoder)
        commonInit()
        updateText()
    }

    override public var text: String? {
        get {
            guard style != nil else {
                return super.text
            }

            return attributedText?.string
        }
        set {
            guard let style else {
                super.text = newValue
                return
            }

            guard let newText = newValue else {
                attributedText = nil
                super.text = nil
                return
            }

            attributedText = style.attributedString(
                from: newText,
                alignment: textAlignment,
                lineBreakMode: lineBreakMode
            )
        }
    }
}

private extension Label {
    func commonInit() {
        font = style?.font
        adjustsFontForContentSizeCategory = true
    }

    func updateText() {
        text = super.text
    }

    func applyTypography() {
        if let typographyRegistration {
            unregisterForTraitChanges(typographyRegistration)
            self.typographyRegistration = nil
        }

        guard let typography else {
            style = nil
            return
        }

        typographyRegistration = bindAppTypography { label in
            let spec = typography.resolvedSpec
            label.style = LabelStyle(
                font: .app(typography),
                lineHeight: spec.lineHeight,
                tracking: spec.tracking
            )
        }
    }
}
