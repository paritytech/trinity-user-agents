import DesignSystem
import UIKit
internal import SnapKit

public struct SearchContactSectionHeaderConfiguration: HashableContentConfiguration {
    public let title: String

    public init(title: String) {
        self.title = title
    }

    public func makeContentView() -> any UIView & UIContentView {
        SearchContactSectionHeaderView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }
}

final class SearchContactSectionHeaderView: UIView, UIContentView {
    private let titleLabel: Label = create {
        $0.typography = .titleMedium
        $0.textColor = UIColor.fgSecondary
        $0.numberOfLines = 1
    }

    private var appliedConfiguration: SearchContactSectionHeaderConfiguration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: SearchContactSectionHeaderConfiguration) {
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
        addSubview(titleLabel)
        titleLabel.snp.makeConstraints {
            $0.leading.equalToSuperview()
            $0.trailing.equalToSuperview()
            $0.top.bottom.equalToSuperview()
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? SearchContactSectionHeaderConfiguration else { return }
        appliedConfiguration = configuration
        titleLabel.text = configuration.title
    }
}
