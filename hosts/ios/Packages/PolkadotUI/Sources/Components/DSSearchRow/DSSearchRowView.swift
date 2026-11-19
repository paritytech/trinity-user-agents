import DesignSystem
import ExternalAccessibility
import UIKit
internal import SnapKit
internal import UIKit_iOS

/// Search row for the scan panel: an edge-to-edge capsule with search field and image.
public final class DSSearchRowView: UIView {
    private let fieldMaterial = DSTabBarFieldMaterialView(cornerRadius: 24)

    private let searchRowGroup: UIStackView = create {
        $0.axis = .horizontal
        $0.spacing = 8
        $0.alignment = .center
    }

    let searchImageView: UIImageView = create {
        $0.image = UIImage(resource: .search18).withRenderingMode(.alwaysTemplate)
        $0.tintColor = .fgPrimary
        $0.contentMode = .scaleAspectFit
    }

    public let searchField: UITextField = create {
        $0.setContentHuggingPriority(.fittingSizeLevel, for: .horizontal)
        $0.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        $0.clearButtonMode = .never

        $0.autocapitalizationType = .none
        $0.autocorrectionType = .no
        $0.textContentType = .nickname
        $0.smartQuotesType = .no
        $0.smartDashesType = .no
        $0.spellCheckingType = .no

        $0.tintColor = .bgAccent
    }

    private let clearButton: UIButton = {
        let button = UIButton()
        let circle = UIView()
        let glyph = UIImageView(
            image: UIImage(
                systemName: "xmark",
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 9, weight: .bold)
            )
        )
        circle.backgroundColor = .bgActionSecondary
        circle.layer.cornerRadius = 11
        circle.isUserInteractionEnabled = false
        glyph.tintColor = .fgPrimary
        button.isHidden = true
        button.addSubview(circle)
        circle.addSubview(glyph)
        circle.snp.makeConstraints {
            $0.center.equalToSuperview()
            $0.size.equalTo(22)
        }
        glyph.snp.makeConstraints { $0.center.equalToSuperview() }
        button.accessibilityLabel = String(localized: .searchContactFieldClear)
        return button
    }()

    private var unfocusedConstraints: [Constraint] = []
    private var focusedConstraints: [Constraint] = []

    public var searchHandler: ((String?) -> Void)?

    override public init(frame: CGRect) {
        super.init(frame: frame)
        applyTextAttributes()
        setupViews()
        setupHandlers()
        applyAccessibilityBindings()
        registerForTraitChanges([DSThemeTrait.self, DSTypographyTrait.self]) { (view: DSSearchRowView, _) in
            view.applyTextAttributes()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Swaps the layout without animating. The caller animates the layout pass.
    public func setSearchFocused(_ focused: Bool) {
        setConstraints(unfocusedConstraints, active: !focused)
        setConstraints(focusedConstraints, active: focused)
    }

    private func setupViews() {
        addSubview(fieldMaterial)
        addSubview(searchRowGroup)
        addSubview(clearButton)

        fieldMaterial.snp.makeConstraints {
            $0.leading.equalToSuperview()
            $0.top.equalToSuperview().offset(8)
            $0.bottom.equalToSuperview().inset(8)
            $0.trailing.equalToSuperview()
            $0.height.equalTo(48)
        }

        searchRowGroup.addArrangedSubview(searchImageView)
        searchRowGroup.addArrangedSubview(searchField)
        searchRowGroup.snp.makeConstraints {
            $0.centerY.equalTo(fieldMaterial)
        }

        clearButton.snp.makeConstraints {
            $0.size.equalTo(32)
            $0.centerY.equalTo(fieldMaterial)
            $0.trailing.equalTo(fieldMaterial).inset(8)
        }

        let placeholderWidth = ceil(searchField.attributedPlaceholder?.size().width ?? 0)
        unfocusedConstraints = searchImageView.snp.prepareConstraints { $0.width.height.equalTo(20) }
            + searchField.snp.prepareConstraints { $0.width.equalTo(placeholderWidth) }
            + searchRowGroup.snp.prepareConstraints {
                $0.centerX.equalTo(fieldMaterial)
                $0.leading.greaterThanOrEqualTo(fieldMaterial).offset(12)
            }
        focusedConstraints = searchImageView.snp.prepareConstraints { $0.width.height.equalTo(22) }
            + searchRowGroup.snp.prepareConstraints {
                $0.leading.equalTo(fieldMaterial).offset(12)
                $0.trailing.equalTo(clearButton.snp.leading)
            }

        setSearchFocused(false)
    }

    private func setupHandlers() {
        searchField.addTarget(self, action: #selector(searchChanged), for: .editingChanged)
        searchField.addTarget(
            self,
            action: #selector(updateClearVisibility),
            for: [.editingDidBegin, .editingDidEnd]
        )
        clearButton.addTarget(self, action: #selector(clearTapped), for: .touchUpInside)

        let focusGesture = UITapGestureRecognizer(target: self, action: #selector(focusSearchField))
        focusGesture.delegate = self
        addGestureRecognizer(focusGesture)
    }

    @objc private func searchChanged() {
        updateClearVisibility()
        searchHandler?(searchField.text)
    }
}

// MARK: - UIGestureRecognizerDelegate

extension DSSearchRowView: UIGestureRecognizerDelegate {
    public func gestureRecognizer(_: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        let touchedView = touch.view
        let touchedField = touchedView?.isDescendant(of: searchField) ?? false
        let touchedClear = touchedView?.isDescendant(of: clearButton) ?? false
        return !touchedField && !touchedClear
    }
}

// MARK: - Private functions

private extension DSSearchRowView {
    /// The field is only as wide as its placeholder in the Scan state, so a tap anywhere on the capsule focuses it.
    @objc func focusSearchField() {
        searchField.becomeFirstResponder()
    }

    @objc func clearTapped() {
        searchField.text = ""
        searchHandler?("")
        updateClearVisibility()
    }

    @objc func updateClearVisibility() {
        let hasText = !(searchField.text ?? "").isEmpty
        clearButton.isHidden = !(searchField.isEditing && hasText)
    }

    /// Text attributes hold a resolved colour and font, so they are rebuilt when the theme or the typography changes.
    func applyTextAttributes() {
        searchField.defaultTextAttributes = Self.labelStyle(.bodyLarge).attributes(
            textColor: UIColor.fgPrimary.resolvedColor(with: traitCollection)
        )
        searchField.attributedPlaceholder = NSAttributedString(
            string: String(localized: .searchContactFieldPlaceholder),
            attributes: Self.labelStyle(.bodyMedium).attributes(
                textColor: UIColor.fgSecondary.resolvedColor(with: traitCollection)
            )
        )
    }

    static func labelStyle(_ typography: TypographyStyle) -> LabelStyle {
        let spec = typography.resolvedSpec
        return LabelStyle(font: .app(typography), lineHeight: spec.lineHeight, tracking: spec.tracking)
    }

    func setConstraints(_ constraints: [Constraint], active: Bool) {
        constraints.forEach { active ? $0.activate() : $0.deactivate() }
    }
}

// MARK: - AccessibilityBound

extension DSSearchRowView: AccessibilityBound {
    public var accessibilityBindings: [AccessibilityBinding] {
        [
            .init(searchField, AccessibilityID.Chats.newChatUsernameInput)
        ]
    }
}
