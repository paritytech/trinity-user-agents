import SwiftUI
import UIKit

internal import SnapKit

/// UIKit host for ``DSAsyncAvatar`` in search result rows.
public final class DSSearchAvatarView: UIView {
    public var viewModel: AvatarViewModel {
        get { model.viewModel }
        set { model.viewModel = newValue }
    }

    /// An optional image loaded over the contact's placeholder.
    public var icon: (any ImageViewModelProtocol)? {
        get { model.icon }
        set { model.icon = newValue }
    }

    override public var intrinsicContentSize: CGSize {
        CGSize(width: DSLetterAvatar.Size.s40.dimension, height: DSLetterAvatar.Size.s40.dimension)
    }

    private let model: Model

    public init() {
        model = Model(viewModel: .colored(text: "", colorSeed: ""))
        super.init(frame: .zero)
        setupContentView()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - Private functions

private extension DSSearchAvatarView {
    func setupContentView() {
        let configuration = UIHostingConfiguration { [model] in
            HostedAvatar(model: model)
        }
        .margins(.all, 0)

        let contentView = configuration.makeContentView()
        contentView.backgroundColor = .clear

        addSubview(contentView)
        contentView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }
}

// MARK: - Model

private extension DSSearchAvatarView {
    @Observable
    final class Model {
        var viewModel: AvatarViewModel
        var icon: (any ImageViewModelProtocol)?

        init(viewModel: AvatarViewModel) {
            self.viewModel = viewModel
        }
    }

    struct HostedAvatar: View {
        let model: Model

        var body: some View {
            DSAsyncAvatar(placeholder: model.viewModel, icon: model.icon, size: .s40)
        }
    }
}
