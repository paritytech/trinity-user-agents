import SwiftUI
import UIKit

internal import SnapKit

/// UIKit host for ``DSSearchAvatar`` in search result rows.
public final class DSSearchAvatarView: UIView {
    public var viewModel: AvatarViewModel {
        get { model.viewModel }
        set { model.viewModel = newValue }
    }

    override public var intrinsicContentSize: CGSize {
        CGSize(width: DSSearchAvatar.dimension, height: DSSearchAvatar.dimension)
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

        init(viewModel: AvatarViewModel) {
            self.viewModel = viewModel
        }
    }

    struct HostedAvatar: View {
        let model: Model

        var body: some View {
            DSSearchAvatar(viewModel: model.viewModel)
        }
    }
}
