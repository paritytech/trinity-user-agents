import UIKit
import DesignSystem
internal import SnapKit

struct PolkadotFooterActionsConfiguration: HashableContentConfiguration {
    let title: String
    let actions: [ChatSystemMessageConfiguration]

    init(title: String, actions: [ChatSystemMessageConfiguration]) {
        self.title = title
        self.actions = actions
    }

    func makeContentView() -> UIView & UIContentView {
        PolkadotFooterActionsContentView(configuration: self)
    }

    func updated(for _: UIConfigurationState) -> PolkadotFooterActionsConfiguration {
        self
    }
}

final class PolkadotFooterActionsContentView: UIView, UIContentView {
    typealias Configuration = PolkadotFooterActionsConfiguration

    private let titleLabel: Label = .create { view in
        view.textColor = .fgSecondary
        view.typography = .bodyMedium
        view.textAlignment = .center
    }

    private let labelContainer: UIView = .create { view in
        view.backgroundColor = .bgSurfaceContainer
    }

    private let contentContainer: UIStackView = .create { view in
        view.spacing = 8
        view.axis = .vertical
    }

    private let actionContainer: UIStackView = .create { view in
        view.spacing = 8
        view.axis = .vertical
        view.isLayoutMarginsRelativeArrangement = true
        view.layoutMargins = .init(top: 0, left: 16, bottom: 0, right: 16)
    }

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

        contentContainer.addArrangedSubview(labelContainer)
        contentContainer.addArrangedSubview(actionContainer)

        labelContainer.addSubview(titleLabel)
        titleLabel.snp.makeConstraints { make in
            make.edges.equalToSuperview().inset(6)
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard
            let configuration = any as? Configuration
        else {
            return
        }
        appliedConfiguration = configuration

        titleLabel.text = configuration.title
        labelContainer.setHidden(configuration.title.isEmpty)

        actionContainer.subviews.forEach { $0.removeFromSuperview() }

        appliedConfiguration.actions.forEach {
            actionContainer.addArrangedSubview($0.makeContentView())
        }

        setHidden(configuration.title.isEmpty && configuration.actions.isEmpty)
    }
}
