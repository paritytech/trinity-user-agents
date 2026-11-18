import UIKit
internal import SnapKit

public final class ChatInputHostView: UIView {
    private enum Replacement {
        /// How long the chat inset and the slot's height take to follow a replacement.
        static let duration: TimeInterval = 0.25
        /// How long each input fades; the outgoing one starts after `fadeOutDelay`, so the slot
        /// never looks empty halfway through.
        static let fadeDuration: TimeInterval = 0.15
        static let fadeOutDelay: TimeInterval = 0.1
        /// The longest a replacement waits for its content before it shows anyway.
        static let contentWait: TimeInterval = 3
    }

    private var currentContentView: UIView?
    private var currentConfiguration: (any ChatInputViewConfigurationProtocol)?
    private let safeAreaFillView = UIView()

    /// An animated replacement waiting for its content: invisible, bottom-aligned at its own height.
    private var pendingContentView: UIView?
    private var pendingConfiguration: (any ChatInputViewConfigurationProtocol)?
    private var pendingTimeout: DispatchWorkItem?
    /// The input just replaced, pinned at its last height and fading out above its replacement.
    private var outgoingContentView: UIView?
    private weak var inputHandler: ChatInputHandling?
    private weak var keyboardGuide: UILayoutGuide?

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        backgroundColor = .clear

        addSubview(safeAreaFillView)
        safeAreaFillView.snp.makeConstraints { make in
            make.leading.trailing.bottom.equalToSuperview()
            make.top.equalTo(safeAreaLayoutGuide.snp.bottom)
        }
    }

    override public func layoutSubviews() {
        super.layoutSubviews()
        // A SwiftUI card renders after it is bound and resizes this view without telling the handler.
        inputHandler?.chatInputDidChange()
    }

    func clear() {
        cancelPendingReplacement()
        finishOutgoingFade()
        currentContentView?.removeFromSuperview()
        currentContentView = nil
        currentConfiguration = nil
        inputHandler = nil
        safeAreaFillView.backgroundColor = nil
    }

    func bind(
        configuration: any ChatInputViewConfigurationProtocol,
        inputHandler: ChatInputHandling?,
        keyboardGuide: UILayoutGuide
    ) {
        if let pendingConfiguration {
            if pendingConfiguration.equalsTo(configuration: configuration) {
                return
            }
            cancelPendingReplacement()
        }

        if let currentConfiguration, currentConfiguration.equalsTo(configuration: configuration) {
            return
        }

        finishOutgoingFade()
        self.inputHandler = inputHandler
        self.keyboardGuide = keyboardGuide

        let newContentView = configuration.makeContentView(for: inputHandler)

        if let currentContentView,
           currentConfiguration?.animatesReplacement == true,
           configuration.animatesReplacement,
           window != nil {
            stageReplacement(newContentView, configuration: configuration, above: currentContentView)
            return
        }

        currentContentView?.removeFromSuperview()
        install(newContentView, configuration: configuration, keyboardGuide: keyboardGuide)
    }

    public var contentHeight: CGFloat {
        guard let currentContentView, bounds.width > 0 else {
            return 0
        }
        return fittingHeight(of: currentContentView)
    }

    public var contentView: UIView? {
        currentContentView
    }

    public var replyInterface: ChatInputReplyPresenting? {
        currentContentView as? ChatInputReplyPresenting
    }

    public var editInterface: ChatInputEditPresenting? {
        currentContentView as? ChatInputEditPresenting
    }

    public var textInputInterface: ChatTextInputPresenting? {
        currentContentView as? ChatTextInputPresenting
    }
}

// MARK: - Replacement

private extension ChatInputHostView {
    func install(
        _ contentView: UIView,
        configuration: any ChatInputViewConfigurationProtocol,
        keyboardGuide: UILayoutGuide
    ) {
        currentContentView = contentView
        currentConfiguration = configuration
        if contentView.superview !== self {
            addSubview(contentView)
        }

        contentView.snp.remakeConstraints { make in
            make.top.leading.trailing.equalToSuperview()
            make.bottom.equalTo(keyboardGuide.snp.top)
        }

        safeAreaFillView.backgroundColor = configuration.safeAreaFillColor
    }

    /// Lays the new input out invisibly under the current one, where it will end up, and swaps
    /// the two once it shows its content.
    func stageReplacement(
        _ contentView: UIView,
        configuration: any ChatInputViewConfigurationProtocol,
        above currentContentView: UIView
    ) {
        pendingContentView = contentView
        pendingConfiguration = configuration

        contentView.alpha = 0
        contentView.isUserInteractionEnabled = false
        insertSubview(contentView, belowSubview: currentContentView)
        placeBottomAligned(contentView, bottom: currentContentView.frame.maxY)

        let timeout = DispatchWorkItem { [weak self, weak contentView] in
            guard let self, let contentView, contentView === pendingContentView else { return }
            commitReplacement()
        }
        pendingTimeout = timeout
        DispatchQueue.main.asyncAfter(deadline: .now() + Replacement.contentWait, execute: timeout)

        configuration.whenContentReady(contentView) { [weak self, weak contentView] in
            // A turn later, so the content that just arrived is drawn and measured.
            DispatchQueue.main.async {
                guard let self, let contentView, contentView === self.pendingContentView else { return }
                self.commitReplacement()
            }
        }
    }

    func commitReplacement() {
        guard let newContentView = pendingContentView,
              let configuration = pendingConfiguration,
              let keyboardGuide else {
            return
        }
        pendingTimeout?.cancel()
        pendingTimeout = nil
        pendingContentView = nil
        pendingConfiguration = nil

        let oldContentView = currentContentView
        layoutIfNeeded()
        // Where it will sit once the slot has its height: the slot grows or shrinks above it.
        placeBottomAligned(newContentView, bottom: oldContentView?.frame.maxY ?? bounds.height)

        if let oldContentView {
            let oldHeight = oldContentView.bounds.height
            oldContentView.isUserInteractionEnabled = false
            // Off the slot's top, so its height no longer decides the slot's.
            oldContentView.snp.remakeConstraints { make in
                make.leading.trailing.equalToSuperview()
                make.bottom.equalTo(keyboardGuide.snp.top)
                make.height.equalTo(oldHeight).priority(999)
            }
            outgoingContentView = oldContentView
        }

        install(newContentView, configuration: configuration, keyboardGuide: keyboardGuide)
        newContentView.isUserInteractionEnabled = true

        UIView.animate(
            withDuration: Replacement.duration,
            delay: 0,
            options: [.curveEaseInOut, .allowUserInteraction]
        ) {
            // The chat measures the new slot and moves its bottom inset inside this animation.
            self.inputHandler?.chatInputDidChange()
            self.superview?.layoutIfNeeded()
        }
        UIView.animate(
            withDuration: Replacement.fadeDuration,
            delay: 0,
            options: [.curveEaseOut, .allowUserInteraction]
        ) {
            newContentView.alpha = 1
        }
        UIView.animate(
            withDuration: Replacement.fadeDuration,
            delay: Replacement.fadeOutDelay,
            options: [.curveEaseIn, .allowUserInteraction]
        ) {
            oldContentView?.alpha = 0
        } completion: { [weak self, weak oldContentView] _ in
            guard let self, let oldContentView, oldContentView === outgoingContentView else { return }
            finishOutgoingFade()
        }
    }

    func cancelPendingReplacement() {
        pendingTimeout?.cancel()
        pendingTimeout = nil
        pendingContentView?.removeFromSuperview()
        pendingContentView = nil
        pendingConfiguration = nil
    }

    func finishOutgoingFade() {
        outgoingContentView?.removeFromSuperview()
        outgoingContentView = nil
    }

    func placeBottomAligned(_ contentView: UIView, bottom: CGFloat) {
        let height = fittingHeight(of: contentView)
        UIView.performWithoutAnimation {
            contentView.frame = CGRect(x: 0, y: bottom - height, width: bounds.width, height: height)
            contentView.layoutIfNeeded()
        }
    }

    /// The input's height at the slot's width, so a card that wraps its text is measured as drawn.
    func fittingHeight(of contentView: UIView) -> CGFloat {
        guard bounds.width > 0 else {
            return contentView.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize).height
        }
        return contentView.systemLayoutSizeFitting(
            CGSize(width: bounds.width, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .fittingSizeLevel
        ).height
    }
}
