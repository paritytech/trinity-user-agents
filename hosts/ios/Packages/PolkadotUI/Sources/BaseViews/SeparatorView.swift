import UIKit
internal import SnapKit

struct SeparatorContentConfiguration: HashableContentConfiguration {
    var color: UIColor
    var height: CGFloat
    var insets: NSDirectionalEdgeInsets

    func makeContentView() -> any UIView & UIContentView {
        SeparatorContentView(configuration: self)
    }
}

final class SeparatorContentView: UIView, UIContentView {
    private let lineView = UIView()

    private var appliedConfiguration: SeparatorContentConfiguration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: SeparatorContentConfiguration) {
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
        backgroundColor = .clear

        addSubview(lineView)
        lineView.snp.makeConstraints {
            $0.height.equalTo(1)
            $0.leading.equalToSuperview()
            $0.trailing.equalToSuperview()
            $0.top.equalToSuperview()
            $0.bottom.equalToSuperview()
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? SeparatorContentConfiguration else { return }
        appliedConfiguration = configuration
        lineView.backgroundColor = configuration.color

        lineView.snp.updateConstraints {
            $0.height.equalTo(configuration.height)
            $0.leading.equalToSuperview().inset(configuration.insets.leading)
            $0.trailing.equalToSuperview().inset(configuration.insets.trailing)
            $0.top.equalToSuperview().inset(configuration.insets.top)
            $0.bottom.equalToSuperview().inset(configuration.insets.bottom)
        }
    }
}
