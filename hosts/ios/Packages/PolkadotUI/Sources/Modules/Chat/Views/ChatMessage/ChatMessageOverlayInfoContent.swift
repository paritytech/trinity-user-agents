import UIKit
import DesignSystem
internal import SnapKit

// MARK: - Configuration

public struct ChatMessageOverlayInfoViewConfiguration: Hashable {
    public struct IconConfiguration: Hashable {
        public enum IconPosition: Hashable {
            case leading
            case trailing
        }

        public let icon: UIImage
        public let position: IconPosition

        public init(icon: UIImage, position: IconPosition = .leading) {
            self.icon = icon
            self.position = position
        }
    }

    public let icon: IconConfiguration?
    public let title: String
    public let backgroundColor: UIColor

    public init(
        icon: IconConfiguration?,
        title: String,
        backgroundColor: UIColor
    ) {
        self.icon = icon
        self.title = title
        self.backgroundColor = backgroundColor
    }
}

// MARK: - Media factory methods

public extension ChatMessageOverlayInfoViewConfiguration {
    static func mediaUploadQueued() -> Self {
        .init(
            icon: .init(icon: UIImage(resource: .upload), position: .leading),
            title: String(localized: .chatMediaUploadQueued),
            backgroundColor: .bgSurfaceOverlay
        )
    }

    static func mediaUploading(progress: CGFloat) -> Self {
        .init(
            icon: nil,
            title: String(localized: .chatMediaUploading(percent: "\(Int(progress * 100))")),
            backgroundColor: .bgSurfaceOverlay
        )
    }

    static func mediaUploadFailed() -> Self {
        .init(
            icon: .init(icon: UIImage(resource: .exclamationMark), position: .leading),
            title: String(localized: .chatMediaUploadFailed),
            backgroundColor: .bgStatusError
        )
    }

    static func mediaDownloading(progress: CGFloat) -> Self {
        .init(
            icon: nil,
            title: String(localized: .chatMediaDownloading(percent: "\(Int(progress * 100))")),
            backgroundColor: .bgSurfaceOverlay
        )
    }

    static func mediaDownloadFailed() -> Self {
        .init(
            icon: .init(icon: UIImage(resource: .exclamationMark), position: .leading),
            title: String(localized: .chatMediaDownloadFailed),
            backgroundColor: .bgStatusError
        )
    }

    static func mediaDeliveryInProgress(date: String) -> Self {
        .init(
            icon: .init(icon: UIImage(resource: .messagePending), position: .trailing),
            title: date,
            backgroundColor: .bgSurfaceOverlay
        )
    }

    static func mediaDeliverySent(date: String) -> Self {
        .init(
            icon: .init(icon: UIImage(resource: .messageDelivered), position: .trailing),
            title: date,
            backgroundColor: .bgSurfaceOverlay
        )
    }
}

// MARK: - Protocol

public protocol ChatMessageOverlayInfoProviding: AnyObject {
    func startInfoUpdate(onUpdate: @escaping (ChatMessageOverlayInfoViewConfiguration?) -> Void)
    func stopInfoUpdate()
}

// MARK: - Static Provider

public final class StaticChatMessageOverlayInfoProvider: ChatMessageOverlayInfoProviding {
    private let configuration: ChatMessageOverlayInfoViewConfiguration

    public init(_ configuration: ChatMessageOverlayInfoViewConfiguration) {
        self.configuration = configuration
    }

    public func startInfoUpdate(onUpdate: @escaping (ChatMessageOverlayInfoViewConfiguration?) -> Void) {
        onUpdate(configuration)
    }

    public func stopInfoUpdate() {}
}

// MARK: - View

public final class ChatMessageOverlayInfoView: UIView {
    private let backgroundView: UIView = .create { view in
        view.clipsToBounds = true
    }

    private let iconView: UIImageView = .create { view in
        view.contentMode = .scaleAspectFit
        view.tintColor = .fgStaticWhite
        view.setContentHuggingPriority(.required, for: .horizontal)
        view.setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    private let label: Label = .create { view in
        view.typography = .bodySmall.emphasized
        view.textColor = .fgStaticWhite
    }

    private let stack: UIStackView = .create { view in
        view.axis = .horizontal
        view.alignment = .center
        view.spacing = DSSpacings.extraSmall
    }

    // MARK: - State

    private var currentProvider: (any ChatMessageOverlayInfoProviding)?

    // MARK: - Init

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupHierarchy()
        setupConstraints()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    deinit {
        currentProvider?.stopInfoUpdate()
    }

    // MARK: - Layout

    override public func layoutSubviews() {
        super.layoutSubviews()
        backgroundView.layer.cornerRadius = backgroundView.bounds.height / 2
    }

    private func setupHierarchy() {
        addSubview(backgroundView)
        backgroundView.addSubview(stack)

        stack.isLayoutMarginsRelativeArrangement = true

        stack.addArrangedSubview(iconView)
        stack.addArrangedSubview(label)
    }

    private func setupConstraints() {
        backgroundView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        stack.snp.makeConstraints { make in
            make.edges.equalToSuperview().inset(
                UIEdgeInsets(top: 2, left: 4, bottom: 2, right: 4)
            )
        }
        iconView.snp.makeConstraints { make in
            make.width.height.equalTo(12)
        }
    }

    // MARK: - Binding

    public func bind(provider: (any ChatMessageOverlayInfoProviding)?) {
        currentProvider?.stopInfoUpdate()
        currentProvider = provider

        guard let provider else {
            setHidden(true)
            return
        }

        provider.startInfoUpdate { [weak self] configuration in
            guard let self else { return }
            if let configuration {
                apply(configuration: configuration)
                setHidden(false)
            } else {
                setHidden(true)
            }
        }
    }

    // MARK: - Apply

    private func apply(configuration: ChatMessageOverlayInfoViewConfiguration) {
        label.text = configuration.title
        backgroundView.backgroundColor = configuration.backgroundColor

        if let iconConfiguration = configuration.icon {
            iconView.image = iconConfiguration.icon.withRenderingMode(.alwaysTemplate)
            iconView.setHidden(false)
            applyIconPosition(iconConfiguration.position)
        } else {
            iconView.image = nil
            iconView.setHidden(true)
        }

        updateContentInsets(iconConfiguration: configuration.icon)
    }

    private func applyIconPosition(
        _ position: ChatMessageOverlayInfoViewConfiguration.IconConfiguration.IconPosition
    ) {
        let desiredIconIndex =
            switch position {
            case .leading: 0
            case .trailing: 1
            }
        if stack.arrangedSubviews.firstIndex(of: iconView) != desiredIconIndex {
            stack.insertArrangedSubview(iconView, at: desiredIconIndex)
        }
    }

    private func updateContentInsets(
        iconConfiguration: ChatMessageOverlayInfoViewConfiguration.IconConfiguration?
    ) {
        let extraSpacing = DSSpacings.extraSmall
        if let iconConfiguration {
            switch iconConfiguration.position {
            case .leading:
                stack.layoutMargins.left = .zero
                stack.layoutMargins.right = extraSpacing
            case .trailing:
                stack.layoutMargins.right = .zero
                stack.layoutMargins.left = extraSpacing
            }
        } else {
            stack.layoutMargins.right = extraSpacing
            stack.layoutMargins.left = extraSpacing
        }
    }
}

#if DEBUG
    private func makeOverlayInfoPreview(
        _ configuration: ChatMessageOverlayInfoViewConfiguration
    ) -> UIView {
        let container = UIView()
        container.backgroundColor = UIColor(resource: .backgroundTertiary)
        let view = ChatMessageOverlayInfoView()
        view.bind(provider: StaticChatMessageOverlayInfoProvider(configuration))
        container.addSubview(view)
        view.snp.makeConstraints { make in
            make.center.equalToSuperview()
        }
        return container
    }

    #Preview("Upload Queued", traits: .fixedLayout(width: 240, height: 60)) {
        makeOverlayInfoPreview(.mediaUploadQueued())
    }

    #Preview("Uploading", traits: .fixedLayout(width: 240, height: 60)) {
        makeOverlayInfoPreview(.mediaUploading(progress: 0.45))
    }

    #Preview("Upload Failed", traits: .fixedLayout(width: 240, height: 60)) {
        makeOverlayInfoPreview(.mediaUploadFailed())
    }

    #Preview("Downloading", traits: .fixedLayout(width: 240, height: 60)) {
        makeOverlayInfoPreview(.mediaDownloading(progress: 0.75))
    }

    #Preview("Download Failed", traits: .fixedLayout(width: 240, height: 60)) {
        makeOverlayInfoPreview(.mediaDownloadFailed())
    }

    #Preview("Delivery In Progress", traits: .fixedLayout(width: 240, height: 60)) {
        makeOverlayInfoPreview(.mediaDeliveryInProgress(date: "22:33"))
    }

    #Preview("Delivery Sent", traits: .fixedLayout(width: 240, height: 60)) {
        makeOverlayInfoPreview(.mediaDeliverySent(date: "22:33"))
    }
#endif
