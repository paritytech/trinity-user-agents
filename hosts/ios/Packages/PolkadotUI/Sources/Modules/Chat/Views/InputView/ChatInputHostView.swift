import UIKit
internal import SnapKit

public final class ChatInputHostView: UIView {
    /// How long a slot that crossfades takes to replace the one before it.
    static let crossfadeDuration: TimeInterval = 0.25

    private var currentContentView: UIView?
    /// The slot a crossfade is fading out, removed when the fade ends.
    private var fadingContentView: UIView?
    private var currentConfiguration: (any ChatInputViewConfigurationProtocol)?
    private let safeAreaFillView = UIView()
    private weak var inputHandler: ChatInputHandling?

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
        dropFadingContentView()
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
        if let currentConfiguration, currentConfiguration.equalsTo(configuration: configuration) {
            return
        }

        let newContentView = configuration.makeContentView(for: inputHandler)
        let crossfades = currentConfiguration.map { configuration.crossfades(from: $0) } ?? false
            && !UIAccessibility.isReduceMotionEnabled

        dropFadingContentView()
        let previousContentView = currentContentView
        if !crossfades {
            previousContentView?.removeFromSuperview()
        }

        currentContentView = newContentView
        addSubview(newContentView)

        currentConfiguration = configuration
        self.inputHandler = inputHandler

        newContentView.snp.makeConstraints { make in
            make.top.leading.trailing.equalToSuperview()
            make.bottom.equalTo(keyboardGuide.snp.top)
        }

        safeAreaFillView.backgroundColor = configuration.safeAreaFillColor

        if crossfades, let previousContentView {
            crossfade(from: previousContentView, to: newContentView)
        }
    }

    /// Fades `previous` out over `next`, which fades in; `previous` takes no touches meanwhile.
    private func crossfade(from previous: UIView, to next: UIView) {
        fadingContentView = previous
        previous.isUserInteractionEnabled = false
        bringSubviewToFront(previous)
        next.alpha = 0
        UIView.animate(
            withDuration: Self.crossfadeDuration,
            delay: 0,
            options: [.curveEaseInOut, .allowUserInteraction, .beginFromCurrentState],
            animations: {
                next.alpha = 1
                previous.alpha = 0
            },
            completion: { [weak self, weak previous] _ in
                guard let self, let previous, fadingContentView === previous else { return }
                dropFadingContentView()
            }
        )
    }

    private func dropFadingContentView() {
        fadingContentView?.layer.removeAllAnimations()
        fadingContentView?.removeFromSuperview()
        fadingContentView = nil
        currentContentView?.layer.removeAllAnimations()
        currentContentView?.alpha = 1
    }

    public var contentHeight: CGFloat {
        guard let currentContentView, bounds.width > 0 else {
            return 0
        }
        return currentContentView.systemLayoutSizeFitting(
            CGSize(width: bounds.width, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .fittingSizeLevel
        ).height
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
