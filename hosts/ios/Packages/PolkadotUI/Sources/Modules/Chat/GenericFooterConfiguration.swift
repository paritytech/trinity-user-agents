import UIKit
internal import UIKit_iOS
internal import SnapKit

// MARK: - GenericFooterConfiguration

public struct GenericFooterConfiguration: HashableContentConfiguration {
    let faqConfiguration: (any HashableContentConfiguration)?

    let contentConfiguration: any HashableContentConfiguration

    public init(
        faqConfiguration: (any HashableContentConfiguration)? = nil,
        contentConfiguration: any HashableContentConfiguration
    ) {
        self.faqConfiguration = faqConfiguration
        self.contentConfiguration = contentConfiguration
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(AnyHashable(contentConfiguration))
        faqConfiguration.map { hasher.combine(AnyHashable($0)) }
    }

    public static func == (
        lhs: GenericFooterConfiguration,
        rhs: GenericFooterConfiguration
    ) -> Bool {
        let faqEqual: Bool =
            switch (lhs.faqConfiguration, rhs.faqConfiguration) {
            case (nil, nil):
                true
            case let (lhsFaq?, rhsFaq?):
                AnyHashable(lhsFaq) == AnyHashable(rhsFaq)
            default:
                false
            }

        return faqEqual && AnyHashable(lhs.contentConfiguration) == AnyHashable(rhs.contentConfiguration)
    }

    public func makeContentView() -> UIView & UIContentView {
        GenericFooterView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> GenericFooterConfiguration {
        self
    }
}

// MARK: - GenericFooterView

final class GenericFooterView: UIView, UIContentView {
    typealias Configuration = GenericFooterConfiguration

    // Container for vertical stacking (FAQ + Content)
    private let contentContainer: UIStackView = .create { view in
        view.spacing = 8
        view.axis = .vertical
    }

    private var faqView: (UIView & UIContentView)?
    private var mainContentView: (UIView & UIContentView)?

    private var appliedConfiguration: Configuration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: Configuration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        addSubview(contentContainer)
        contentContainer.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard
            let configuration = any as? Configuration
        else {
            return
        }
        appliedConfiguration = configuration

        let mainConfig = appliedConfiguration.contentConfiguration

        if let existingMainView = mainContentView {
            existingMainView.configuration = mainConfig
        } else {
            let newMainView = mainConfig.makeContentView()
            contentContainer.addArrangedSubview(newMainView)
            mainContentView = newMainView
        }

        guard let faqConfig = appliedConfiguration.faqConfiguration else {
            faqView?.removeFromSuperview()
            faqView = nil
            return
        }

        if let existingFaqView = faqView {
            existingFaqView.configuration = faqConfig
        } else {
            let newFaqView = faqConfig.makeContentView()
            contentContainer.insertArrangedSubview(newFaqView, at: 0)
            contentContainer.setCustomSpacing(16, after: newFaqView)
            faqView = newFaqView
        }
    }
}
