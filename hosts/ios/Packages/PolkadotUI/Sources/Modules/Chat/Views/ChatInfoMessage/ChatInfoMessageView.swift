import DesignSystem
import UIKit
internal import SnapKit

public struct ChatInfoMessageConfiguration: HashableContentConfiguration {
    init(
        attributedText: NSAttributedString,
        textBackgroundConfiguration: BackgroundConfiguration = .empty,
        contentInsets: NSDirectionalEdgeInsets = .zero,
        showDividers: Bool = false
    ) {
        self.attributedText = attributedText
        self.textBackgroundConfiguration = textBackgroundConfiguration
        self.contentInsets = contentInsets
        self.showDividers = showDividers
    }

    let attributedText: NSAttributedString
    let textBackgroundConfiguration: BackgroundConfiguration
    let contentInsets: NSDirectionalEdgeInsets
    let showDividers: Bool

    public func makeContentView() -> any UIView & UIContentView {
        ChatInfoMessageView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }
}

extension ChatInfoMessageConfiguration {
    struct BackgroundConfiguration: Hashable {
        let color: UIColor?
        let cornerRadius: CGFloat
        let insets: NSDirectionalEdgeInsets

        static var empty: Self {
            .init(color: .clear, cornerRadius: 0, insets: .zero)
        }
    }
}

final class ChatInfoMessageView: UIView, UIContentView {
    private let contentContainer = UIView()

    private let titleLabel: UILabel = create {
        $0.numberOfLines = 0
        $0.textAlignment = .center
    }

    private let titleBackgroundView: UIView = create {
        $0.setHidden(true)
    }

    private let dividerLine: UIView = create {
        $0.backgroundColor = UIColor.fgPrimary
        $0.setHidden(true)
    }

    private var appliedConfiguration: ChatInfoMessageConfiguration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: ChatInfoMessageConfiguration) {
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
        Self.layoutFittingCompressedSize
    }

    private func setupViews() {
        addSubview(contentContainer)
        contentContainer.snp.makeConstraints {
            $0.leading.equalToSuperview()
                .offset(appliedConfiguration.contentInsets.leading)
            $0.trailing.equalToSuperview()
                .inset(appliedConfiguration.contentInsets.trailing)
            $0.top.equalToSuperview()
                .offset(appliedConfiguration.contentInsets.top)
            $0.bottom.equalToSuperview()
                .inset(appliedConfiguration.contentInsets.bottom)
        }

        contentContainer.addSubview(dividerLine)
        contentContainer.addSubview(titleBackgroundView)
        contentContainer.addSubview(titleLabel)

        titleLabel.snp.makeConstraints {
            $0.center.equalToSuperview()
            $0.top.equalToSuperview()
                .offset(appliedConfiguration.textBackgroundConfiguration.insets.top)
            $0.bottom.equalToSuperview()
                .inset(appliedConfiguration.textBackgroundConfiguration.insets.bottom)
            // with equal centering there is no need for trailing constraint
            $0.leading.greaterThanOrEqualToSuperview().inset(DSSpacings.large)
        }

        titleBackgroundView.snp.makeConstraints {
            $0.top.bottom.equalTo(titleLabel)
            $0.leading.equalTo(titleLabel).offset(-8)
            $0.trailing.equalTo(titleLabel).offset(8)
        }

        dividerLine.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview()
            $0.centerY.equalTo(titleLabel)
            $0.height.equalTo(1)
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? ChatInfoMessageConfiguration else { return }
        appliedConfiguration = configuration

        titleLabel.attributedText = configuration.attributedText
        titleLabel.snp.updateConstraints {
            $0.top.equalToSuperview()
                .offset(appliedConfiguration.textBackgroundConfiguration.insets.top)
            $0.bottom.equalToSuperview()
                .inset(appliedConfiguration.textBackgroundConfiguration.insets.bottom)
        }

        contentContainer.backgroundColor = configuration.textBackgroundConfiguration.color
        contentContainer.layer.cornerRadius = configuration.textBackgroundConfiguration.cornerRadius

        dividerLine.setHidden(!configuration.showDividers)
        titleBackgroundView.setHidden(!configuration.showDividers)
        titleBackgroundView.backgroundColor = UIColor.bgSurfaceMain

        contentContainer.snp.updateConstraints {
            $0.leading.equalToSuperview()
                .offset(appliedConfiguration.contentInsets.leading)
            $0.trailing.equalToSuperview()
                .inset(appliedConfiguration.contentInsets.trailing)
            $0.top.equalToSuperview()
                .offset(appliedConfiguration.contentInsets.top)
            $0.bottom.equalToSuperview()
                .inset(appliedConfiguration.contentInsets.bottom)
        }
    }
}

#Preview(traits: .fixedLayout(width: 300, height: 150)) {
    ChatInfoMessageConfiguration.youAdded(username: "usernameusernameusernameusernameusernameusernameusername.77")
        .makeContentView()
}

#Preview(traits: .fixedLayout(width: 300, height: 150)) {
    ChatInfoMessageConfiguration.youAdded(by: "username.88")
        .makeContentView()
}
