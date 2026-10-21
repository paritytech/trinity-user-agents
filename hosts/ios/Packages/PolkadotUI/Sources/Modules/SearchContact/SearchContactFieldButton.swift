import DesignSystem
import UIKit
internal import SnapKit
internal import UIKit_iOS

public final class SearchContactFieldButton: UIControl {
    private let materialView = DSTabBarFieldMaterialView(cornerRadius: 24)

    private let iconImageView: UIImageView = create {
        $0.image = UIImage(resource: .search18).withRenderingMode(.alwaysTemplate)
        $0.tintColor = .fgPrimary
        $0.isUserInteractionEnabled = false
        $0.snp.makeConstraints {
            $0.width.height.equalTo(20)
        }
    }

    private let label: Label = create {
        $0.typography = .bodyMedium
        $0.textColor = .fgSecondary
        $0.isUserInteractionEnabled = false
        $0.text = String(localized: .searchContactFieldPlaceholder)
    }

    private let contentStack: UIStackView = create {
        $0.axis = .horizontal
        $0.spacing = 8
        $0.alignment = .center
        $0.isUserInteractionEnabled = false
    }

    public var onTap: (() -> Void)?

    public init() {
        super.init(frame: .zero)

        setupLayout()
        setupAccessibility()

        addTarget(self, action: #selector(didTap), for: .touchUpInside)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension SearchContactFieldButton {
    func setupLayout() {
        addSubview(materialView)
        addSubview(contentStack)
        contentStack.addArrangedSubview(iconImageView)
        contentStack.addArrangedSubview(label)

        snp.makeConstraints {
            $0.height.equalTo(48)
        }

        materialView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        contentStack.snp.makeConstraints {
            $0.center.equalToSuperview()
        }
    }

    func setupAccessibility() {
        isAccessibilityElement = true
        accessibilityTraits = .button
        accessibilityLabel = String(localized: .searchContactFieldPlaceholder)
    }

    @objc func didTap() {
        onTap?()
    }
}
