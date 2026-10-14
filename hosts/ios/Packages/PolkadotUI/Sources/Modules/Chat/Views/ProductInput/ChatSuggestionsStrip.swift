import DesignSystem
import UIKit
internal import SnapKit

/// The quick replies of a product room: chips in a horizontal strip above the input.
final class ChatSuggestionsStrip: UIView {
    private let scrollView = UIScrollView()
    private let stack: UIStackView = create {
        $0.axis = .horizontal
        $0.spacing = DSSpacings.extraSmall
    }

    var onSelect: ((ChatRoomAccessoryModel.Suggestion) -> Void)?

    private(set) var suggestions: [ChatRoomAccessoryModel.Suggestion] = []

    override init(frame: CGRect) {
        super.init(frame: frame)

        scrollView.showsHorizontalScrollIndicator = false
        addSubview(scrollView)
        scrollView.addSubview(stack)
        scrollView.snp.makeConstraints {
            $0.edges.equalToSuperview()
            $0.height.equalTo(36)
        }
        stack.snp.makeConstraints {
            $0.edges.equalToSuperview().inset(UIEdgeInsets(top: 0, left: DSSpacings.medium, bottom: 0, right: 16))
            $0.height.equalToSuperview()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func bind(_ suggestions: [ChatRoomAccessoryModel.Suggestion]) {
        guard suggestions != self.suggestions else { return }
        self.suggestions = suggestions

        stack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for suggestion in suggestions {
            stack.addArrangedSubview(makeChip(suggestion))
        }
    }

    private func makeChip(_ suggestion: ChatRoomAccessoryModel.Suggestion) -> UIButton {
        var configuration = UIButton.Configuration.filled()
        configuration.title = suggestion.text
        configuration.cornerStyle = .capsule
        configuration.baseBackgroundColor = .bgActionSecondary
        configuration.baseForegroundColor = .fgPrimary
        configuration.contentInsets = NSDirectionalEdgeInsets(top: 6, leading: 12, bottom: 6, trailing: 12)

        let button = UIButton(configuration: configuration, primaryAction: UIAction { [weak self] _ in
            self?.onSelect?(suggestion)
        })
        button.accessibilityLabel = suggestion.text
        return button
    }
}
