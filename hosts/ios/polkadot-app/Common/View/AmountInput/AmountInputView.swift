import UIKit
import FoundationExt
import UIKit_iOS
import DesignSystem
import PolkadotUI

class AmountInputView: UIControl {
    let symbolLabel: UILabel = .create { label in
        label.font = UIFont.displayExtraLarge
        label.textColor = .fgSecondary
    }

    let unitLabel: UILabel = .create { label in
        label.font = UIFont.app(.smallCapsHeadlineMedium).withSize(UIFont.displayExtraLarge.pointSize)
        label.textColor = .fgSecondary
    }

    let textField: UITextField = .create { textField in
        textField.apply(style: .init(
            font: UIFont.displayExtraLarge,
            textColor: .fgPrimary,
            tintColor: .fgPrimary
        ))

        textField.attributedPlaceholder = NSAttributedString(
            string: "0",
            attributes: [
                .foregroundColor: UIColor.fgTertiary,
                .font: UIFont.displayExtraLarge
            ]
        )

        textField.keyboardType = .decimalPad
    }

    var minFontSize: CGFloat = 24.0 {
        didSet {
            setNeedsLayout()
        }
    }

    var maxFontSize: CGFloat = UIFont.displayExtraLarge.pointSize {
        didSet {
            setNeedsLayout()
        }
    }

    var horizontalSpacing: CGFloat = 8.0 {
        didSet {
            setNeedsLayout()
        }
    }

    private(set) var inputViewModel: AmountInputViewModelProtocol?
    private(set) var isSymbolInFront: Bool = false

    var completed: Bool {
        if let inputViewModel {
            inputViewModel.isValid
        } else {
            false
        }
    }

    var hasValidNumber: Bool {
        inputViewModel?.decimalAmount != nil
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        configure()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func bind(assetViewModel: AssetAmountViewModel) {
        symbolLabel.text = assetViewModel.symbol
        isSymbolInFront = assetViewModel.isSymbolInFront

        setNeedsLayout()
    }

    func bind(unit: String?) {
        unitLabel.text = unit

        setNeedsLayout()
    }

    func bind(inputViewModel: AmountInputViewModelProtocol) {
        self.inputViewModel?.observable.remove(observer: self)
        inputViewModel.observable.add(observer: self)

        self.inputViewModel = inputViewModel

        textField.text = inputViewModel.displayAmount

        setNeedsLayout()
    }

    // MARK: Layout

    override func layoutSubviews() {
        super.layoutSubviews()

        layoutContent()
    }

    private var amountText: String {
        textField.text?.nilIfEmpty ?? "0"
    }

    private var symbolText: String {
        symbolLabel.text ?? ""
    }

    private var unitText: String {
        unitLabel.text ?? ""
    }

    private func unitFont(for size: CGFloat) -> UIFont {
        UIFont.app(.smallCapsHeadlineMedium).withSize(size)
    }

    private func unitSpacing(for font: UIFont) -> CGFloat {
        " ".estimateWidth(for: font, height: bounds.height)
    }

    private func contentWidth(for font: UIFont) -> CGFloat {
        var width = amountText.estimateWidth(for: font, height: bounds.height)

        if !symbolText.isEmpty {
            width += symbolText.estimateWidth(for: font, height: bounds.height) + horizontalSpacing
        }

        if !unitText.isEmpty {
            width += unitSpacing(for: font)
                + unitText.estimateWidth(for: unitFont(for: font.pointSize), height: bounds.height)
        }

        return width
    }

    private func fittingFont(named fontName: String) -> UIFont? {
        var fontSize = maxFontSize

        while fontSize > minFontSize {
            guard let font = UIFont(name: fontName, size: fontSize) else {
                return nil
            }

            if contentWidth(for: font) <= bounds.width {
                return font
            }

            fontSize -= 1.0
        }

        return UIFont(name: fontName, size: minFontSize)
    }

    private func layoutContent() {
        guard let font = fittingFont(named: UIFont.displayExtraLarge.fontName) else {
            return
        }

        symbolLabel.font = font
        textField.font = font
        unitLabel.font = unitFont(for: font.pointSize)

        let symbolWidth = symbolText.isEmpty ? 0 : symbolLabel.intrinsicContentSize.width
        let symbolGap = symbolText.isEmpty ? 0 : horizontalSpacing
        let unitWidth = unitText.isEmpty ? 0 : unitLabel.intrinsicContentSize.width
        let unitGap = unitText.isEmpty ? 0 : unitSpacing(for: font)
        let amountWidth = max(
            min(contentWidth(for: font), bounds.width) - symbolWidth - symbolGap - unitWidth - unitGap,
            0
        )

        let totalWidth = symbolWidth + symbolGap + amountWidth + unitGap + unitWidth
        var cursorX = bounds.midX - totalWidth / 2.0

        if isSymbolInFront {
            cursorX = place(symbolLabel, width: symbolWidth, at: cursorX, baselineOf: font) + symbolGap
            cursorX = place(textField, width: amountWidth, at: cursorX, baselineOf: font)
        } else {
            cursorX = place(textField, width: amountWidth, at: cursorX, baselineOf: font) + symbolGap
            cursorX = place(symbolLabel, width: symbolWidth, at: cursorX, baselineOf: font)
        }

        place(unitLabel, width: unitWidth, at: cursorX + unitGap, baselineOf: font)
    }

    @discardableResult
    private func place(_ view: UIView, width: CGFloat, at originX: CGFloat, baselineOf font: UIFont) -> CGFloat {
        let height = view.intrinsicContentSize.height
        let amountTop = bounds.midY - font.lineHeight / 2.0
        let baselineY = amountTop + font.ascender
        let viewFont = (view as? UILabel)?.font ?? font

        view.frame = CGRect(
            x: originX,
            y: baselineY - viewFont.ascender - (height - viewFont.lineHeight) / 2.0,
            width: width,
            height: height
        )

        return originX + width
    }

    // MARK: Configure

    private func configure() {
        backgroundColor = UIColor.clear

        configureContentViewIfNeeded()
        configureLocalHandlers()
        configureTextFieldHandlers()
    }

    private func configureLocalHandlers() {
        addTarget(self, action: #selector(actionTouchUpInside), for: .touchUpInside)
    }

    private func configureTextFieldHandlers() {
        textField.delegate = self
    }

    private func configureContentViewIfNeeded() {
        addSubview(textField)
        addSubview(symbolLabel)
        addSubview(unitLabel)
    }

    // MARK: Action

    @objc private func actionTouchUpInside() {
        textField.becomeFirstResponder()
    }
}

extension AmountInputView: UITextFieldDelegate {
    func textField(
        _: UITextField,
        shouldChangeCharactersIn range: NSRange,
        replacementString string: String
    ) -> Bool {
        inputViewModel?.didReceiveReplacement(string, for: range) ?? false
    }

    func textFieldDidBeginEditing(_: UITextField) {
        sendActions(for: .editingDidBegin)
    }
}

extension AmountInputView: AmountInputViewModelObserver {
    func amountInputDidChange() {
        textField.text = inputViewModel?.displayAmount

        sendActions(for: .editingChanged)

        setNeedsLayout()
    }
}
