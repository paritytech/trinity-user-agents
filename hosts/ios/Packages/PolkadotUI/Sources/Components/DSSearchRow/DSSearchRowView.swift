import DesignSystem
import ExternalAccessibility
import UIKit
internal import SnapKit
internal import UIKit_iOS

/// Search row for the scan panel: an edge-to-edge capsule with search field and image.
public final class DSSearchRowView: UIView {
    private let searchCapsuleContainer = {
        let view = DSChatInputGlassBackground(
            cornerRadius: 24,
            fallbackColor: .bgSurfaceContainer,
            interactive: false
        )
        view.snp.makeConstraints {
            $0.height.equalTo(48)
        }

        return view
    }()

    let searchImageView: UIImageView = create {
        $0.image = UIImage(resource: .search18).withRenderingMode(.alwaysTemplate)
        $0.tintColor = UIColor.fgSecondary
        $0.snp.makeConstraints {
            $0.width.height.equalTo(18)
        }
    }

    public let searchField: UITextField = create {
        var defaultTextAttributes = LabelStyle.body14Regular().attributes()
        defaultTextAttributes[.foregroundColor] = UIColor.fgPrimary

        var placeholderAttributes = defaultTextAttributes
        placeholderAttributes[.foregroundColor] = UIColor.fgDisabled

        $0.defaultTextAttributes = defaultTextAttributes
        $0.attributedPlaceholder = NSAttributedString(
            string: String(localized: .searchContactFieldPlaceholder),
            attributes: placeholderAttributes
        )

        $0.setContentHuggingPriority(.fittingSizeLevel, for: .horizontal)
        $0.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        $0.clearButtonMode = .whileEditing

        $0.autocapitalizationType = .none
        $0.autocorrectionType = .no
        $0.textContentType = .nickname
        $0.smartQuotesType = .no
        $0.smartDashesType = .no
        $0.spellCheckingType = .no

        $0.tintColor = UIColor.fgPrimary
    }

    public var searchHandler: ((String?) -> Void)?

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
        setupHandlers()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        addSubview(searchCapsuleContainer)

        searchCapsuleContainer.snp.makeConstraints {
            $0.leading.equalToSuperview()
            $0.top.equalToSuperview().offset(8)
            $0.bottom.equalToSuperview().inset(8)
            $0.trailing.equalToSuperview()
        }

        searchCapsuleContainer.addSubview(searchImageView)
        searchCapsuleContainer.addSubview(searchField)

        searchImageView.snp.makeConstraints {
            $0.leading.equalToSuperview().offset(12)
            $0.centerY.equalToSuperview()
        }

        searchField.snp.makeConstraints {
            $0.leading.equalTo(searchImageView.snp.trailing).offset(8)
            $0.centerY.equalToSuperview()
            $0.trailing.equalToSuperview().inset(16)
        }
    }

    private func setupHandlers() {
        searchField.delegate = self
        searchField.addTarget(self, action: #selector(searchChanged), for: .editingChanged)
    }

    @objc private func searchChanged() {
        searchHandler?(searchField.text)
    }
}

// MARK: - UITextFieldDelegate

extension DSSearchRowView: UITextFieldDelegate {
    /// The clear button does not raise `editingChanged`, so the search is reset here.
    public func textFieldShouldClear(_: UITextField) -> Bool {
        searchHandler?("")

        return true
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
