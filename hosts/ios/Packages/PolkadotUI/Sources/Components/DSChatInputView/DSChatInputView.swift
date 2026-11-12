import DesignSystem
import ExternalAccessibility
import UIKit
internal import SnapKit
internal import UIKit_iOS

final class DSChatInputView: UIView {
    private enum Metrics {
        static let fieldCornerRadius: CGFloat = DSRadii.large
        static let sendButtonSize: CGFloat = 32
        static let leadingButtonSize: CGFloat = 48
        static let outerPadding: CGFloat = DSSpacings.small
        static let stackSpacing: CGFloat = DSSpacings.small
        static let fieldLeading: CGFloat = DSSpacings.mediumIncreased
        static let textVerticalInset: CGFloat = DSSpacings.extraMedium
        static let counterTopSpacing: CGFloat = DSSpacings.extraTiny
        static let counterBottomInset: CGFloat = DSSpacings.small
        static let minHeight: CGFloat = 64
    }

    private enum BannerState {
        case none
        case reply
        case edit
    }

    private let containerView = withGlassContainer(UIView(), cornerRadius: Metrics.fieldCornerRadius)

    let textView = UITextView()
    private let placeholderLabel = Label()
    private let characterCounterLabel = Label()
    private let sendButton: DSIconButton = .chatInputSend
    private let attachmentButton = DSIconButton.chatInputLeading(icon: UIImage(resource: .icon24Plus))
    private let transferButton = DSIconButton.chatInputLeading(icon: UIImage(resource: .icon24Dollar))

    private lazy var attachmentItem = Self.withGlassContainer(
        attachmentButton,
        cornerRadius: Metrics.leadingButtonSize / 2
    )
    private lazy var transferItem = Self.withGlassContainer(transferButton, cornerRadius: Metrics.leadingButtonSize / 2)

    private let horizontalStack: UIStackView = .create {
        $0.axis = .horizontal
        $0.spacing = Metrics.stackSpacing
        $0.alignment = .bottom
    }

    private let bannerView = DSChatInputReplyBanner()
    private var bannerState: BannerState = .none

    private var appliedConfiguration: ChatInputViewConfiguration
    weak var inputHandler: ChatInputHandling?

    // MARK: Layout state

    private var textViewTopConstraint: Constraint?
    private var textViewTrailingToButton: Constraint?
    private var textViewHeightConstraint: Constraint?
    private var textViewMaxHeight: CGFloat = 0
    private var textViewStartsScrolling = false {
        didSet {
            updateTextViewScrollingDependentConstraints()
        }
    }

    private let growthAnimator: BlockViewAnimatorProtocol = BlockViewAnimator(
        duration: 0.15,
        delay: 0,
        options: [.curveLinear]
    )
    private let showBannerAnimator: BlockViewAnimatorProtocol = BlockViewAnimator(
        duration: 0.25,
        delay: 0,
        options: [.curveEaseOut]
    )
    private let hideBannerAnimator: BlockViewAnimatorProtocol = BlockViewAnimator(
        duration: 0.25,
        delay: 0,
        options: [.curveEaseInOut]
    )

    private var shouldShowSend: Bool {
        appliedConfiguration.canSendWithoutText || !trimmedText().isEmpty
    }

    override var intrinsicContentSize: CGSize {
        let stackHeight = horizontalStack.systemLayoutSizeFitting(
            CGSize(width: bounds.width - 2 * Metrics.outerPadding, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .fittingSizeLevel
        ).height
        return CGSize(
            width: UIView.noIntrinsicMetric,
            height: max(Metrics.minHeight, stackHeight + 2 * Metrics.outerPadding)
        )
    }

    init(configuration: ChatInputViewConfiguration, handler: ChatInputHandling?) {
        appliedConfiguration = configuration
        inputHandler = handler
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
        textView.accessibilityId(rawValue: configuration.inputAccessibilityId)
        sendButton.accessibilityId(rawValue: configuration.sendAccessibilityId)
        attachmentButton.accessibilityId(AccessibilityID.Chat.attachButton)
        transferButton.accessibilityId(AccessibilityID.Chat.sendFundsButton)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - Setup

private extension DSChatInputView {
    func setupViews() {
        backgroundColor = .clear

        configureTextView()
        configurePlaceholder()
        configureCharacterCounter()

        let tap = UITapGestureRecognizer(target: self, action: #selector(focusText))
        // Don't cancel touches in subviews, otherwise the gesture swallows the send
        // button's .touchUpInside (it lives inside this container; the +/$ buttons don't).
        tap.cancelsTouchesInView = false
        containerView.isUserInteractionEnabled = true
        containerView.addGestureRecognizer(tap)
        containerView.clipsToBounds = true

        sendButton.onTap = { [weak self] in self?.sendButtonTapped() }
        transferButton.onTap = { [weak self] in self?.transferButtonTapped() }
        attachmentButton.onTap = { [weak self] in self?.attachmentButtonTapped() }

        if #available(iOS 26.0, *) {
            let containerEffect = UIGlassContainerEffect()
            containerEffect.spacing = Metrics.stackSpacing
            let glassContainer = UIVisualEffectView(effect: containerEffect)
            addSubview(glassContainer)
            glassContainer.snp.makeConstraints { $0.edges.equalToSuperview() }
            glassContainer.contentView.addSubview(horizontalStack)
        } else {
            addSubview(horizontalStack)
        }
        horizontalStack.addArrangedSubview(containerView)

        let containerContentView: UIView =
            // on iOS 26+ it will be UIVisualEffectView, but before it regular UIView
            // its required to add subviews to effectView's contentView
            if let effectView = containerView as? UIVisualEffectView {
                effectView.contentView
            } else {
                containerView
            }

        containerContentView.addSubview(bannerView)
        containerContentView.addSubview(textView)
        containerContentView.addSubview(placeholderLabel)
        containerContentView.addSubview(sendButton)

        if appliedConfiguration.showsCharacterCounter {
            containerContentView.addSubview(characterCounterLabel)
        }

        bannerView.alpha = 0
        bannerView.onClose = { [weak self] in
            self?.hideBanner(notifyCallback: true)
        }

        setupConstraints()
        textView.delegate = self
    }

    static func withGlassContainer(_ view: UIView, cornerRadius: CGFloat) -> UIView {
        guard #available(iOS 26.0, *) else {
            let container = UIView()
            container.backgroundColor = .bgActionSecondary
            container.layer.cornerRadius = cornerRadius
            container.clipsToBounds = true
            container.addSubview(view)
            view.snp.makeConstraints { $0.directionalEdges.equalToSuperview() }
            return container
        }
        let effect = UIGlassEffect()
        effect.isInteractive = true
        let effectView = UIVisualEffectView(effect: effect)
        effectView.cornerConfiguration = .corners(radius: .fixed(cornerRadius))
        effectView.contentView.addSubview(view)
        view.snp.makeConstraints { $0.edges.equalToSuperview() }
        return effectView
    }

    func configureTextView() {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byWordWrapping
        paragraph.minimumLineHeight = 20
        paragraph.maximumLineHeight = 20
        textView.typingAttributes = [
            .font: UIFont.paragraphLarge,
            .foregroundColor: UIColor.fgPrimary,
            .paragraphStyle: paragraph
        ]
        textView.textColor = .fgPrimary
        textView.tintColor = .fgPrimary
        textView.isScrollEnabled = false
        textView.backgroundColor = .clear
        textView.textContainerInset = .zero
        textView.textContainer.lineFragmentPadding = 0
        textView.returnKeyType = .default
        textView.smartInsertDeleteType = .no
        textView.setContentCompressionResistancePriority(.required, for: .vertical)
        textView.setContentHuggingPriority(.required, for: .vertical)
    }

    func configurePlaceholder() {
        placeholderLabel.typography = .paragraphLarge
        placeholderLabel.textColor = UIColor.fgDisabled
        placeholderLabel.numberOfLines = 1
    }

    func configureCharacterCounter() {
        characterCounterLabel.typography = .captionMedium
        characterCounterLabel.textColor = UIColor.fgTertiary
        characterCounterLabel.textAlignment = .right
    }

    func setupConstraints() {
        bannerView.snp.makeConstraints {
            $0.top.leading.trailing.equalToSuperview()
        }

        horizontalStack.snp.makeConstraints {
            $0.top.greaterThanOrEqualToSuperview().offset(Metrics.outerPadding).priority(.high)
            $0.bottom.equalToSuperview().inset(Metrics.outerPadding).priority(.required)
            $0.leading.equalToSuperview().offset(Metrics.outerPadding)
            $0.trailing.equalToSuperview().inset(Metrics.outerPadding)
        }

        textView.snp.makeConstraints {
            textViewTopConstraint = $0.top.equalTo(bannerView.snp.bottom)
                .offset(Metrics.textVerticalInset)
                .constraint
            if appliedConfiguration.showsCharacterCounter {
                $0.bottom.equalTo(characterCounterLabel.snp.top).offset(-Metrics.counterTopSpacing)
            } else {
                $0.bottom.equalToSuperview().inset(Metrics.textVerticalInset)
            }
            $0.leading.equalToSuperview().offset(Metrics.fieldLeading)

            textViewTrailingToButton = $0.trailing.equalTo(sendButton.snp.leading)
                .offset(-Metrics.outerPadding)
                .constraint
            $0.trailing.equalToSuperview()
                .offset(-Metrics.fieldLeading)
                .priority(.medium)

            textViewHeightConstraint = $0.height.equalTo(44).constraint
            textViewHeightConstraint?.deactivate()
        }

        placeholderLabel.snp.makeConstraints {
            $0.leading.equalTo(textView).offset(2)
            $0.centerY.equalTo(textView)
            $0.trailing.lessThanOrEqualTo(textView)
        }

        if appliedConfiguration.showsCharacterCounter {
            characterCounterLabel.snp.makeConstraints {
                $0.leading.greaterThanOrEqualTo(textView)
                $0.trailing.equalTo(textView)
                $0.bottom.equalToSuperview().inset(Metrics.counterBottomInset)
            }
        }

        sendButton.snp.makeConstraints {
            $0.size.equalTo(Metrics.sendButtonSize)
            $0.trailing.equalToSuperview().inset(Metrics.outerPadding)
            $0.bottom.equalToSuperview().inset(Metrics.outerPadding)
            $0.top.greaterThanOrEqualToSuperview().offset(Metrics.outerPadding)
        }
    }
}

// MARK: - Configuration

private extension DSChatInputView {
    func apply(_ configuration: ChatInputViewConfiguration) {
        if let currentText = textView.text, currentText.count > configuration.maxCharacterCount {
            textView.text = String(currentText.prefix(configuration.maxCharacterCount))
        }

        appliedConfiguration = configuration
        placeholderLabel.text = configuration.placeholder

        updateLeadingButtons(
            canPay: configuration.canPay,
            canAttachFile: configuration.canAttachFile
        )

        updateTextViewMaxHeight()
        updateCharacterCounter()
        updatePlaceholderVisibility()
        updateSendButtonState()
        updateTextViewScrolling()
    }

    func updateLeadingButtons(canPay: Bool, canAttachFile: Bool) {
        [
            attachmentButton,
            transferButton
        ].forEach {
            horizontalStack.removeArrangedSubview($0)
            $0.removeFromSuperview()
        }

        if canPay {
            horizontalStack.insertArrangedSubview(transferItem, at: 0)
        }
        if canAttachFile {
            horizontalStack.insertArrangedSubview(attachmentItem, at: 0)
        }
    }

    func updateTextViewMaxHeight() {
        let attributes = textView.typingAttributes
        let paragraph = attributes[.paragraphStyle] as? NSParagraphStyle

        let effectiveLineHeight: CGFloat = {
            let maxLineHeight = paragraph?.maximumLineHeight ?? 0
            if maxLineHeight > 0 { return maxLineHeight }
            let minLineHeight = paragraph?.minimumLineHeight ?? 0
            if minLineHeight > 0 { return minLineHeight }
            let font = (attributes[.font] as? UIFont) ?? textView.font ?? .preferredFont(forTextStyle: .body)
            return font.lineHeight
        }()

        let verticalInsets = textView.textContainerInset.top + textView.textContainerInset.bottom
        let maxNumberOfLines = max(1, appliedConfiguration.maxNumberOfLines)
        let maxFieldHeight = ceil(effectiveLineHeight * CGFloat(maxNumberOfLines) + verticalInsets)

        textViewMaxHeight = maxFieldHeight
        textViewHeightConstraint?.update(offset: maxFieldHeight)
        textViewHeightConstraint?.isActive = textViewStartsScrolling
    }
}

// MARK: - Actions

private extension DSChatInputView {
    @objc func focusText() {
        textView.becomeFirstResponder()
    }

    func sendButtonTapped() {
        guard shouldShowSend else { return }
        send()
    }

    func transferButtonTapped() {
        inputHandler?.chatInputDidTransfer()
    }

    func attachmentButtonTapped() {
        inputHandler?.chatInputDidAttachment()
    }

    func send() {
        let text = trimmedText()
        guard !text.isEmpty || appliedConfiguration.canSendWithoutText else { return }

        textView.text = ""
        updateSendButtonState()
        updateCharacterCounter()
        updatePlaceholderVisibility()
        updateTextViewScrolling()

        inputHandler?.chatInputDidSend(text)
    }
}

// MARK: - UI updates

private extension DSChatInputView {
    func updateSendButtonState() {
        if shouldShowSend {
            sendButton.isHidden = false
            textViewTrailingToButton?.isActive = true
        } else {
            sendButton.isHidden = true
            textViewTrailingToButton?.isActive = false
        }
    }

    func updateCharacterCounter() {
        guard appliedConfiguration.showsCharacterCounter else { return }

        characterCounterLabel.text = "\(textView.text.count)/\(appliedConfiguration.maxCharacterCount)"
    }

    func updatePlaceholderVisibility() {
        let hidden = !textView.text.isEmpty
        guard placeholderLabel.isHidden != hidden else { return }
        UIView.transition(
            with: placeholderLabel,
            duration: hidden ? 0.15 : 0.20,
            options: .transitionCrossDissolve
        ) {
            self.placeholderLabel.isHidden = hidden
        }
    }

    func updateTextViewScrolling() {
        guard textView.bounds.width > 0 else { return }

        guard !textView.text.isEmpty else {
            textViewStartsScrolling = false
            textView.isScrollEnabled = false
            textViewHeightConstraint?.isActive = false
            textView.invalidateIntrinsicContentSize()
            invalidateIntrinsicContentSize()
            return
        }

        let targetSize = CGSize(width: textView.bounds.width, height: .greatestFiniteMagnitude)
        let fittingHeight = textView.sizeThatFits(targetSize).height
        let shouldScroll = fittingHeight >= textViewMaxHeight

        guard textViewStartsScrolling != shouldScroll else { return }
        textViewStartsScrolling = shouldScroll
        textView.isScrollEnabled = shouldScroll
        textViewHeightConstraint?.isActive = shouldScroll

        textView.invalidateIntrinsicContentSize()
        invalidateIntrinsicContentSize()
    }

    func updateTextViewScrollingDependentConstraints() {
        textViewTopConstraint?.update(offset: textViewStartsScrolling ? 0 : Metrics.textVerticalInset)
    }

    func trimmedText() -> String {
        textView.text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

// MARK: - UITextViewDelegate

extension DSChatInputView: UITextViewDelegate {
    func textView(
        _ textView: UITextView,
        shouldChangeTextIn range: NSRange,
        replacementText text: String
    ) -> Bool {
        let currentText = textView.text ?? ""
        let newText = (currentText as NSString).replacingCharacters(in: range, with: text)

        if newText.count > appliedConfiguration.maxCharacterCount {
            handleTextTruncation(textView: textView, range: range, replacementText: text, currentText: currentText)
            return false
        }
        return true
    }

    func textViewDidChange(_: UITextView) {
        updateCharacterCounter()
        updatePlaceholderVisibility()
        updateSendButtonState()
        updateTextViewScrolling()

        growthAnimator.animate(block: {
            self.invalidateIntrinsicContentSize()
            self.superview?.layoutIfNeeded()
        }, completionBlock: nil)

        inputHandler?.chatInputDidChange()
    }
}

// MARK: - Presenting protocols

extension DSChatInputView: ChatTextInputPresenting {
    func activateTextField() {
        textView.becomeFirstResponder()
    }
}

extension DSChatInputView: ChatInputReplyPresenting {
    func showReplyBanner(title: String, messageText: String) {
        bannerState = .reply
        showBanner(title: title, text: messageText, populateTextField: false)
    }

    func hideReplyBanner() {
        hideBanner(notifyCallback: false)
    }
}

extension DSChatInputView: ChatInputEditPresenting {
    var isEditing: Bool {
        if case .edit = bannerState { return true }
        return false
    }

    func showEditBanner(title: String, currentText: String) {
        bannerState = .edit
        showBanner(title: title, text: currentText, populateTextField: true)
    }

    func hideEditBanner() {
        hideBanner(notifyCallback: false)
    }
}

// MARK: - Banner

private extension DSChatInputView {
    func showBanner(title: String, text: String, populateTextField: Bool) {
        bannerView.configure(title: title, messageText: text)
        bannerView.alpha = 0
        bannerView.show(animated: false)

        if populateTextField {
            textView.text = text
            updateCharacterCounter()
            updatePlaceholderVisibility()
            updateSendButtonState()
            updateTextViewScrolling()
        }

        showBannerAnimator.animate(block: {
            self.invalidateIntrinsicContentSize()
            self.bannerView.alpha = 1
            self.superview?.layoutIfNeeded()
        }, completionBlock: nil)

        inputHandler?.chatInputDidChange()
    }

    func hideBanner(notifyCallback: Bool) {
        let wasEditing = isEditing
        let previousState = bannerState
        bannerState = .none

        if wasEditing {
            textView.text = ""
            updateCharacterCounter()
            updatePlaceholderVisibility()
            updateSendButtonState()
            updateTextViewScrolling()
        }

        hideBannerAnimator.animate(block: {
            self.bannerView.alpha = 0
            self.bannerView.hide(animated: false)
            self.invalidateIntrinsicContentSize()
            self.superview?.layoutIfNeeded()
        }, completionBlock: nil)

        inputHandler?.chatInputDidChange()

        guard notifyCallback else { return }
        switch previousState {
        case .reply: inputHandler?.chatInputDidCancelReply()
        case .edit: inputHandler?.chatInputDidCancelEdit()
        case .none: break
        }
    }

    func handleTextTruncation(
        textView: UITextView,
        range: NSRange,
        replacementText: String,
        currentText: String
    ) {
        let remainingCharacters = appliedConfiguration.maxCharacterCount - (currentText.count - range.length)
        guard remainingCharacters > 0 else { return }

        let truncated = String(replacementText.prefix(remainingCharacters))
        let cursorOffset = range.location + truncated.count

        textView.textStorage.replaceCharacters(in: range, with: truncated)

        DispatchQueue.main.async {
            let currentText = textView.text ?? ""
            let safeCursorOffset = min(cursorOffset, currentText.count)
            if let newPosition = textView.position(from: textView.beginningOfDocument, offset: safeCursorOffset) {
                textView.selectedTextRange = textView.textRange(from: newPosition, to: newPosition)
            }
        }

        textViewDidChange(textView)
    }
}

#if DEBUG
    @available(iOS 26.0, *)
    #Preview("Empty") {
        let view = DSChatInputView(
            configuration: .chat(canPay: true, canAttachFile: true),
            handler: nil
        )
        view.backgroundColor = .bgSurfaceMain
        view.textView.text = ""
        return view
    }

    @available(iOS 26.0, *)
    #Preview("Chat request") {
        let view = DSChatInputView(configuration: .chatRequest(), handler: nil)
        view.backgroundColor = .bgSurfaceMain
        view.textView.text = "Hi! I'd like to add you as a contact"
        return view
    }

    @available(iOS 26.0, *)
    #Preview("Reply + text") {
        let view = DSChatInputView(
            configuration: .chat(canPay: true, canAttachFile: true),
            handler: nil
        )
        view.backgroundColor = .bgSurfaceMain
        view.textView.text = ""
        view.showReplyBanner(title: "Reply to person.99", messageText: "How can I assist you today?")
        return view
    }
#endif
