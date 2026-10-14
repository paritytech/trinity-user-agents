import UIKit
internal import SnapKit
import DesignSystem

public final class ChatProductLinkPreviewView: UIView, UIContentView {
    // MARK: - Subviews

    private let thumbnailView: UIImageView = .create { view in
        view.layer.cornerRadius = 7.5
        view.clipsToBounds = true
        view.contentMode = .scaleAspectFill
    }

    private let linkLabel: Label = .create { view in
        view.typography = .bodySmall
        view.text = String(localized: .chatProductLinkLabel)
    }

    private let nameLabel: Label = .create { view in
        view.typography = .bodyLarge
        view.numberOfLines = 1
        view.lineBreakMode = .byTruncatingTail
    }

    private let domainLabel: Label = .create { view in
        view.typography = .bodySmall
        view.numberOfLines = 1
        view.lineBreakMode = .byTruncatingMiddle
    }

    private let textStack: UIStackView = .create { view in
        view.axis = .vertical
        view.alignment = .leading
        view.spacing = 0
    }

    private let horizontalStack: UIStackView = .create { view in
        view.axis = .horizontal
        view.alignment = .center
        view.spacing = DSSpacings.small
    }

    // MARK: - State

    private var appliedConfiguration: ChatProductLinkPreviewConfiguration

    private var previewState = PreviewState()

    public var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    override public var intrinsicContentSize: CGSize {
        CGSize(
            width: UIView.layoutFittingExpandedSize.width,
            height: UIView.layoutFittingCompressedSize.height
        )
    }

    // MARK: - Init

    public init(configuration: ChatProductLinkPreviewConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        addTapGesture()
        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Layout

    private func setupViews() {
        addSubview(horizontalStack)
        horizontalStack.clipsToBounds = true
        horizontalStack.layer.cornerRadius = DSRadii.medium
        horizontalStack.isLayoutMarginsRelativeArrangement = true
        horizontalStack.layoutMargins = UIEdgeInsets(
            top: DSSpacings.small,
            left: DSSpacings.smallIncreased,
            bottom: DSSpacings.small,
            right: DSSpacings.smallIncreased
        )

        horizontalStack.addArrangedSubview(thumbnailView)
        horizontalStack.addArrangedSubview(textStack)

        textStack.addArrangedSubview(linkLabel)
        textStack.addArrangedSubview(nameLabel)
        textStack.addArrangedSubview(domainLabel)

        horizontalStack.snp.makeConstraints { make in
            make.directionalEdges.equalToSuperview()
        }

        thumbnailView.snp.makeConstraints { make in
            make.width.height.equalTo(60)
        }
    }

    private func addTapGesture() {
        let recognizer = UITapGestureRecognizer(target: self, action: #selector(handleTap))
        addGestureRecognizer(recognizer)
        isUserInteractionEnabled = true
    }

    @objc private func handleTap() {
        appliedConfiguration.tap()
    }

    // MARK: - Apply

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? ChatProductLinkPreviewConfiguration else { return }

        let newId = configuration.nameProvider?.identifier ?? configuration.domain
        if previewState.needsUpdate(providerId: newId) {
            appliedConfiguration.nameProvider?.cancel()
            appliedConfiguration.imageViewModel?.cancel(on: thumbnailView)
            previewState.reset()
            previewState.providerId = newId
            nameLabel.text = nil
            thumbnailView.image = nil
        }

        appliedConfiguration = configuration

        applyStyle(configuration.style)
        domainLabel.text = configuration.domain
        setHidden(false)

        requestPreviewIfNeeded()
    }

    private func applyStyle(_ style: ChatProductLinkPreviewConfiguration.Style) {
        switch style {
        case .inbox:
            linkLabel.textColor = .fgSecondaryInverted
            nameLabel.textColor = .fgPrimaryInverted
            domainLabel.textColor = .fgPrimaryInverted
            horizontalStack.backgroundColor = .bgSurfaceNestedInverted
            thumbnailView.backgroundColor = .bgSurfaceContainerInverted
        case .outbox:
            linkLabel.textColor = .fgSecondary
            nameLabel.textColor = .fgPrimary
            domainLabel.textColor = .fgPrimary
            horizontalStack.backgroundColor = .bgSurfaceNested
            thumbnailView.backgroundColor = .bgSurfaceContainer
        }
    }

    private func requestPreviewIfNeeded() {
        guard !previewState.hasRequested else { return }
        previewState.hasRequested = true

        requestName()
        requestImage()
    }

    private func requestName() {
        guard let provider = appliedConfiguration.nameProvider else { return }

        provider.provideName { [weak self, weak provider] name in
            guard
                let self,
                let provider,
                previewState.providerId == provider.identifier
            else {
                return
            }

            guard let name else {
                setHidden(true)
                return
            }

            nameLabel.text = name
        }
    }

    private func requestImage() {
        guard let imageViewModel = appliedConfiguration.imageViewModel else { return }
        imageViewModel.loadImage(
            on: thumbnailView,
            targetSize: thumbnailView.frame.size,
            animated: true
        )
    }
}

private extension ChatProductLinkPreviewView {
    struct PreviewState {
        var providerId: String?
        var hasRequested: Bool = false

        mutating func reset() {
            providerId = nil
            hasRequested = false
        }

        func needsUpdate(providerId: String?) -> Bool {
            self.providerId != providerId
        }
    }
}

#if DEBUG
    #Preview {
        class StaticNameProvider: ChatProductNameProviding {
            let identifier: String

            func provideName(_ completion: @escaping (String?) -> Void) {
                completion(identifier)
            }

            func cancel() {}

            init(name: String) {
                identifier = name
            }
        }

        let stack = UIStackView(arrangedSubviews: [
            ChatProductLinkPreviewConfiguration(
                domain: "Domain.aaa",
                style: .inbox,
                nameProvider: StaticNameProvider(name: "Product name"),
                imageViewModel: nil, // StaticImagePreviewProvider(image: .add),
                tap: {}
            ).makeContentView(),

            ChatProductLinkPreviewConfiguration(
                domain: "Domain.aaa",
                style: .outbox,
                nameProvider: StaticNameProvider(name: "Product name"),
                imageViewModel: nil, // StaticImagePreviewProvider(image: .add),
                tap: {}
            ).makeContentView()
        ])
        stack.axis = .vertical
        stack.spacing = 8
        return stack
    }
#endif
