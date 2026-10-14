import UIKit
import DesignSystem
internal import SnapKit

public struct ChatMessageStatusViewConfiguration: HashableContentConfiguration {
    private enum Constants {
        static let stackSpacing: CGFloat = DSSpacings.tiny
        static let editedLabelSpacing: CGFloat = DSSpacings.extraSmall
    }

    public struct Background: Hashable {
        public enum Shape: Hashable {
            case capsule
        }

        public let color: UIColor
        public let shape: Shape
        public let contentInsets: UIEdgeInsets

        public init(
            color: UIColor,
            shape: Shape = .capsule,
            contentInsets: UIEdgeInsets = .zero
        ) {
            self.color = color
            self.shape = shape
            self.contentInsets = contentInsets
        }

        public static func == (lhs: Background, rhs: Background) -> Bool {
            lhs.color == rhs.color &&
                lhs.shape == rhs.shape &&
                lhs.contentInsets == rhs.contentInsets
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(color)
            hasher.combine(shape)
            hasher.combine(contentInsets.top)
            hasher.combine(contentInsets.left)
            hasher.combine(contentInsets.bottom)
            hasher.combine(contentInsets.right)
        }
    }

    struct AutoupdateMode: Hashable {
        let date: Date?
        let formatter: TimestampFormatting

        var text: String? {
            date.map(formatter.string(for:))
        }

        static func == (lhs: AutoupdateMode, rhs: AutoupdateMode) -> Bool {
            lhs.date == rhs.date
        }

        func hash(into hasher: inout Hasher) {
            hasher.combine(date)
        }
    }

    enum Mode: Hashable {
        case autoupdate(AutoupdateMode)
        case concrete(String)

        var text: String? {
            switch self {
            case let .autoupdate(model):
                model.text
            case let .concrete(text):
                text
            }
        }
    }

    let mode: Mode
    let textColor: UIColor
    let image: UIImage?
    let isEdited: Bool
    let background: Background?

    public init(
        dateFormatter: TimestampFormatting,
        date: Date?,
        textColor: UIColor,
        image: UIImage?,
        isEdited: Bool,
        background: Background? = nil
    ) {
        mode = .autoupdate(.init(date: date, formatter: dateFormatter))
        self.textColor = textColor
        self.image = image
        self.isEdited = isEdited
        self.background = background
    }

    public init(
        timestampText: String,
        textColor: UIColor,
        image: UIImage?,
        isEdited: Bool,
        background: Background? = nil
    ) {
        mode = .concrete(timestampText)
        self.textColor = textColor
        self.image = image
        self.isEdited = isEdited
        self.background = background
    }

    public func makeContentView() -> any UIView & UIContentView {
        ChatMessageStatusView(configuration: self)
    }

    public static func == (
        lhs: ChatMessageStatusViewConfiguration,
        rhs: ChatMessageStatusViewConfiguration
    ) -> Bool {
        lhs.mode == rhs.mode &&
            lhs.textColor == rhs.textColor &&
            lhs.image == rhs.image &&
            lhs.isEdited == rhs.isEdited &&
            lhs.background == rhs.background
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(mode)
        hasher.combine(textColor)
        hasher.combine(image)
        hasher.combine(isEdited)
        hasher.combine(background)
    }

    /// Estimates the width needed for the status view (timestamp + checkmark + edited label)
    var estimatedWidth: CGFloat {
        var width: CGFloat = 0
        let font = UIFont.systemFont(ofSize: 12)

        // Edited label width
        if isEdited {
            let editedText = String(localized: .messageEdited)
            let editedWidth = (editedText as NSString).size(withAttributes: [.font: font]).width
            width += (editedWidth + Constants.editedLabelSpacing)
        }

        // Timestamp width
        if let timeString = mode.text {
            let timeWidth = (timeString as NSString).size(withAttributes: [.font: font]).width
            width += (timeWidth + Constants.stackSpacing)
        }

        // Checkmark image width
        if let image {
            width += (image.size.width + Constants.stackSpacing)
        }

        if let background {
            width += background.contentInsets.left + background.contentInsets.right
        }

        return width
    }

    public var placeholderImage: UIImage? {
        guard estimatedWidth > 0 else {
            return nil
        }
        let padding: CGFloat = 8
        let verticalInsets = (background?.contentInsets.top ?? 0) + (background?.contentInsets.bottom ?? 0)
        let size = CGSize(width: estimatedWidth + padding, height: 14 + verticalInsets)
        let renderer = UIGraphicsImageRenderer(size: size)
        return renderer.image { _ in }
    }
}

final class ChatMessageStatusView: UIView, UIContentView {
    let stackView = UIStackView()

    let editedLabel: Label = create {
        $0.typography = .bodySmall.emphasized
        $0.textAlignment = .right
        $0.text = String(localized: .messageEdited)
        $0.setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    let label: Label = create {
        $0.typography = .bodySmall.emphasized
        $0.textAlignment = .right
        $0.setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    let imageView: UIImageView = create {
        $0.setContentCompressionResistancePriority(.required, for: .horizontal)
        $0.setContentCompressionResistancePriority(.required, for: .vertical)
        $0.setContentHuggingPriority(.required, for: .horizontal)
        $0.setContentHuggingPriority(.required, for: .vertical)
        $0.contentMode = .scaleAspectFit
    }

    private var backgroundView: UIView?

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    var appliedConfiguration: ChatMessageStatusViewConfiguration

    init(configuration: ChatMessageStatusViewConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var intrinsicContentSize: CGSize {
        guard !stackView.subviews.allSatisfy(\.isHidden) else {
            return .zero
        }
        return super.intrinsicContentSize
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        updateBackgroundIfNeeded()
    }

    func setupViews() {
        stackView.axis = .horizontal
        stackView.alignment = .center
        stackView.isLayoutMarginsRelativeArrangement = true
        stackView.directionalLayoutMargins = .zero

        stackView.addArrangedSubview(editedLabel)
        stackView.addArrangedSubview(label)
        stackView.addArrangedSubview(imageView)

        stackView.setCustomSpacing(DSSpacings.extraSmall, after: editedLabel)
        stackView.setCustomSpacing(DSSpacings.tiny, after: label)

        imageView.snp.makeConstraints {
            $0.width.equalTo(imageView.snp.height)
            $0.width.equalTo(12)
        }

        addSubview(stackView)
        stackView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }
    }
}

private extension ChatMessageStatusView {
    func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? ChatMessageStatusViewConfiguration else { return }
        let dateText = configuration.mode.text

        appliedConfiguration = configuration
        imageView.image = configuration.image
        imageView.tintColor = configuration.textColor
        label.text = dateText
        label.textColor = configuration.textColor

        editedLabel.setHidden(!configuration.isEdited)
        editedLabel.textColor = configuration.textColor

        imageView.setHidden(configuration.image == nil)
        label.setHidden(dateText == nil)

        applyBackground(configuration.background)
    }

    func applyBackground(_ background: ChatMessageStatusViewConfiguration.Background?) {
        if let background {
            let layerView = backgroundView ?? makeBackgroundLayerView()
            layerView.backgroundColor = background.color
            stackView.directionalLayoutMargins = NSDirectionalEdgeInsets(
                top: background.contentInsets.top,
                leading: background.contentInsets.left,
                bottom: background.contentInsets.bottom,
                trailing: background.contentInsets.right
            )
            setNeedsLayout()
        } else {
            backgroundView?.removeFromSuperview()
            backgroundView = nil
            stackView.directionalLayoutMargins = .zero
        }
    }

    func updateBackgroundIfNeeded() {
        guard let layerView = backgroundView,
              let configuration = appliedConfiguration.background else {
            return
        }
        switch configuration.shape {
        case .capsule:
            layerView.layer.cornerRadius = layerView.bounds.height / 2
        }
    }

    func makeBackgroundLayerView() -> UIView {
        let view = UIView()
        view.clipsToBounds = true
        insertSubview(view, at: 0)
        view.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }
        backgroundView = view
        return view
    }
}
