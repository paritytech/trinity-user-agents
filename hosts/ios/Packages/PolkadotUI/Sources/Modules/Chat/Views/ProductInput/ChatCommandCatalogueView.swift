import DesignSystem
import UIKit
internal import SnapKit

/// The slash commands that match what the user types after `/`, above the input. A pick completes the command.
final class ChatCommandCatalogueView: UIView {
    private let stack: UIStackView = create {
        $0.axis = .vertical
        $0.spacing = 0
    }

    var onPick: ((ChatRoomAccessoryModel.Command) -> Void)?

    private(set) var commands: [ChatRoomAccessoryModel.Command] = []

    /// The catalogue shows at most this number of commands. The user types more to narrow it.
    private static let visibleLimit = 5

    override init(frame: CGRect) {
        super.init(frame: frame)

        backgroundColor = .bgSurfaceMain
        layer.cornerRadius = 12
        clipsToBounds = true
        addSubview(stack)
        stack.snp.makeConstraints { $0.edges.equalToSuperview().inset(DSSpacings.extraSmall) }
        isHidden = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func bind(_ commands: [ChatRoomAccessoryModel.Command]) {
        guard commands != self.commands else { return }
        self.commands = commands
        isHidden = commands.isEmpty

        stack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for command in commands.prefix(Self.visibleLimit) {
            stack.addArrangedSubview(makeRow(command))
        }
    }

    private func makeRow(_ command: ChatRoomAccessoryModel.Command) -> UIButton {
        var configuration = UIButton.Configuration.plain()
        configuration.title = "/" + command.name + (command.argumentHint.map { " " + $0 } ?? "")
        configuration.subtitle = command.description
        configuration.titleAlignment = .leading
        configuration.baseForegroundColor = .fgPrimary
        configuration.contentInsets = NSDirectionalEdgeInsets(top: 6, leading: 8, bottom: 6, trailing: 8)

        let button = UIButton(configuration: configuration, primaryAction: UIAction { [weak self] _ in
            self?.onPick?(command)
        })
        button.contentHorizontalAlignment = .leading
        button.accessibilityLabel = "/" + command.name
        return button
    }
}
