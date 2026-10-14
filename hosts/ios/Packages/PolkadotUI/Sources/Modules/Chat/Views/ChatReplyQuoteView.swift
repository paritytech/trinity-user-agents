import UIKit
import DesignSystem
internal import SnapKit

final class ChatReplyQuoteView: UIView {
    enum Style {
        case inbox
        case outbox
    }

    private let verticalAccent: UIView = create {
        $0.backgroundColor = .strokeTertiary
        $0.layer.cornerRadius = 2
    }

    private let usernameLabel: Label = create {
        $0.typography = .titleTiny
        $0.numberOfLines = 1
    }

    private let quoteLabel: Label = create {
        $0.typography = .bodySmall
        $0.numberOfLines = 2
    }

    private var style: Style = .outbox

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(username: String, preview: String, style: Style) {
        self.style = style

        applyStyle()

        usernameLabel.text = username

        let cleanPreview = preview
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\n", with: " ")

        quoteLabel.text = cleanPreview
    }
}

// MARK: - Private functions

extension ChatReplyQuoteView {
    private func applyStyle() {
        switch style {
        case .inbox:
            backgroundColor = .bgSurfaceNested
            usernameLabel.textColor = .fgPrimary
            quoteLabel.textColor = .fgPrimary

        case .outbox:
            backgroundColor = .bgSurfaceNestedInverted
            usernameLabel.textColor = .fgPrimaryInverted
            quoteLabel.textColor = .fgPrimaryInverted
        }
    }

    private func setupViews() {
        setContentHuggingPriority(.defaultLow, for: .horizontal)
        setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        layer.cornerRadius = DSRadii.medium
        clipsToBounds = true

        addSubview(verticalAccent)

        verticalAccent.snp.makeConstraints {
            $0.leading.equalToSuperview()
            $0.top.bottom.equalToSuperview()
            $0.width.equalTo(4)
        }

        let stackView = UIStackView(arrangedSubviews: [usernameLabel, quoteLabel])
        stackView.axis = .vertical
        stackView.alignment = .leading

        addSubview(stackView)
        stackView.snp.makeConstraints {
            $0.directionalHorizontalEdges.equalToSuperview().inset(DSSpacings.medium)
            $0.directionalVerticalEdges.equalToSuperview().inset(DSSpacings.small)
        }
    }
}
