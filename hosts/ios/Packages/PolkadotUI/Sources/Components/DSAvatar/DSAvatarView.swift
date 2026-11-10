import DesignSystem
import Observation
import SwiftUI
import UIKit

internal import SnapKit

// UIKit wrapper around DSAvatar for view-based layouts. Hosts the SwiftUI
// avatar via UIHostingConfiguration, so no view controller plumbing is needed.
// Property changes mutate an @Observable model the hosted view reads, so
// updates go through normal SwiftUI invalidation instead of re-creating the
// hosting configuration.
public final class DSAvatarView: UIView {
    public var viewModel: AvatarViewModel {
        get { model.viewModel }
        set { model.viewModel = newValue }
    }

    /// Loaded over ``viewModel``, which shows until it arrives.
    public var icon: (any ImageViewModelProtocol)? {
        get { model.icon }
        set { model.icon = newValue }
    }

    public var size: DSLetterAvatar.Size {
        get { model.size }
        set {
            model.size = newValue
            invalidateIntrinsicContentSize()
        }
    }

    // Fixed design dimension for the current size; use instead of hardcoding sizes at call sites.
    public var proposedDimension: CGFloat {
        model.size.dimension
    }

    override public var intrinsicContentSize: CGSize {
        CGSize(width: proposedDimension, height: proposedDimension)
    }

    private let model: DSAvatarView.Model

    public convenience init(size: DSLetterAvatar.Size) {
        self.init(viewModel: .colored(text: "", colorSeed: ""), size: size)
    }

    public init(viewModel: AvatarViewModel, size: DSLetterAvatar.Size) {
        model = Model(viewModel: viewModel, size: size)
        super.init(frame: .zero)
        setupContentView()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - Private functions

extension DSAvatarView {
    private func setupContentView() {
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

private extension DSAvatarView {
    @Observable
    final class Model {
        var viewModel: AvatarViewModel
        var icon: (any ImageViewModelProtocol)?
        var size: DSLetterAvatar.Size

        init(viewModel: AvatarViewModel, size: DSLetterAvatar.Size) {
            self.viewModel = viewModel
            self.size = size
        }
    }

    struct HostedAvatar: View {
        let model: Model

        var body: some View {
            DSAsyncAvatar(placeholder: model.viewModel, icon: model.icon, size: model.size)
        }
    }
}
