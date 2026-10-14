import DesignSystem
import UIKit
internal import SnapKit
internal import UIKit_iOS

public final class SearchContactFieldButton: UIControl {
    private let iconImageView: UIImageView = create {
        $0.image = UIImage(resource: .search18).withRenderingMode(.alwaysTemplate)
        $0.tintColor = .fgSecondary
        $0.isUserInteractionEnabled = false
        $0.snp.makeConstraints {
            $0.width.height.equalTo(18)
        }
    }

    private let label: Label = create {
        $0.style = .body14Regular()
        $0.textColor = .fgDisabled
        $0.isUserInteractionEnabled = false
        $0.text = String(localized: .searchContactFieldPlaceholder)
    }

    public var onTap: (() -> Void)?

    public init() {
        super.init(frame: .zero)

        setupStyle()
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
    func setupStyle() {
        layer.cornerRadius = 24
        layer.cornerCurve = .continuous
        layer.borderWidth = 0.8
        backgroundColor = .bgSurfaceNested
        applyLayerColors()

        registerForTraitChanges([DSThemeTrait.self]) { (view: SearchContactFieldButton, _) in
            view.applyLayerColors()
        }
    }

    func setupLayout() {
        addSubview(iconImageView)
        addSubview(label)

        snp.makeConstraints {
            $0.height.equalTo(48)
        }

        iconImageView.snp.makeConstraints {
            $0.leading.equalToSuperview().inset(12)
            $0.centerY.equalToSuperview()
        }

        label.snp.makeConstraints {
            $0.leading.equalTo(iconImageView.snp.trailing).offset(8)
            $0.trailing.equalToSuperview().inset(16)
            $0.centerY.equalToSuperview()
        }
    }

    func setupAccessibility() {
        isAccessibilityElement = true
        accessibilityTraits = .button
        accessibilityLabel = String(localized: .searchContactFieldPlaceholder)
    }

    func applyLayerColors() {
        layer.borderColor = UIColor.strokePrimary.resolvedColor(with: traitCollection).cgColor
    }

    @objc func didTap() {
        onTap?()
    }
}
