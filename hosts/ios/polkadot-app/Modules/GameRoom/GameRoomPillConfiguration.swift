import UIKit
import SnapKit
import PolkadotUI

struct GameRoomPillConfiguration: HashableContentConfiguration {
    let content: GameRoomPillViewModel.Content
    private let onTap: () -> Void

    init(
        content: GameRoomPillViewModel.Content,
        onTap: @escaping () -> Void
    ) {
        self.content = content
        self.onTap = onTap
    }

    func makeContentView() -> any UIView & UIContentView {
        GameRoomPillContentView(configuration: self)
    }

    static func == (lhs: GameRoomPillConfiguration, rhs: GameRoomPillConfiguration) -> Bool {
        lhs.content == rhs.content
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(content)
    }

    fileprivate func makeViewModel() -> GameRoomPillViewModel {
        let viewModel = GameRoomPillViewModel(content: content)
        viewModel.onTap = onTap

        return viewModel
    }
}

private final class GameRoomPillContentView: UIView, UIContentView {
    private var appliedConfiguration: GameRoomPillConfiguration
    private var swiftUIContentView: (UIView & UIContentView)?

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: GameRoomPillConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? GameRoomPillConfiguration else {
            return
        }

        appliedConfiguration = configuration

        let swiftUIConfiguration = SwiftUIContentConfiguration(
            view: GameRoomPillView(viewModel: configuration.makeViewModel()),
            id: configuration.content
        )

        if let swiftUIContentView {
            swiftUIContentView.configuration = swiftUIConfiguration
            swiftUIContentView.invalidateIntrinsicContentSize()
            invalidateIntrinsicContentSize()
            return
        }

        let contentView = swiftUIConfiguration.makeContentView()
        addSubview(contentView)

        contentView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }

        swiftUIContentView = contentView
        invalidateIntrinsicContentSize()
    }
}
