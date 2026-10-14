import DesignSystem
import Observation
import SwiftUI
import UIKit

internal import SnapKit

// UIKit wrapper around DSButton for view-based layouts. Hosts the SwiftUI
// button via UIHostingConfiguration, so no view controller plumbing is needed.
// Subclasses UIControl and emits .touchUpInside, so both the onTap closure
// and addTarget/addAction wiring work. Property changes mutate an @Observable
// model the hosted view reads, so updates go through normal SwiftUI
// invalidation instead of re-creating the hosting configuration.
public final class DSButtonView: UIControl {
    public var onTap: (() -> Void)?

    public var title: String {
        get { model.title }
        set { model.title = newValue }
    }

    public var style: DSButtonStyle.Style {
        get { model.style }
        set { model.style = newValue }
    }

    public var shape: DSButtonStyle.Shape {
        get { model.shape }
        set { model.shape = newValue }
    }

    public var size: DSButtonStyle.Size {
        get { model.size }
        set { model.size = newValue }
    }

    public var leadingIcon: ImageResource? {
        get { model.leadingIcon }
        set { model.leadingIcon = newValue }
    }

    public var trailingIcon: ImageResource? {
        get { model.trailingIcon }
        set { model.trailingIcon = newValue }
    }

    public var expands: Bool {
        get { model.expands }
        set { model.expands = newValue }
    }

    // Fixed design height for the current size; use instead of hardcoding heights at call sites.
    public var proposedHeight: CGFloat {
        model.size.height
    }

    override public var isEnabled: Bool {
        didSet { model.isEnabled = isEnabled }
    }

    private let model: DSButtonView.Model

    public convenience init() {
        self.init("")
    }

    public init(
        _ title: String,
        style: DSButtonStyle.Style = .primary,
        shape: DSButtonStyle.Shape = .pill,
        size: DSButtonStyle.Size = .large,
        leadingIcon: ImageResource? = nil,
        trailingIcon: ImageResource? = nil,
        expands: Bool = false
    ) {
        model = Model(
            title: title,
            style: style,
            shape: shape,
            size: size,
            leadingIcon: leadingIcon,
            trailingIcon: trailingIcon,
            expands: expands
        )
        super.init(frame: .zero)
        model.action = { [weak self] in
            self?.onTap?()
            self?.sendActions(for: .touchUpInside)
        }
        setupContentView()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func setTitle(_ title: String) {
        self.title = title
    }
}

// MARK: - Private functions

extension DSButtonView {
    private func setupContentView() {
        let configuration = UIHostingConfiguration { [model] in
            HostedButton(model: model)
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

private extension DSButtonView {
    @Observable
    final class Model {
        var title: String
        var style: DSButtonStyle.Style
        var shape: DSButtonStyle.Shape
        var size: DSButtonStyle.Size
        var leadingIcon: ImageResource?
        var trailingIcon: ImageResource?
        var expands: Bool
        var isEnabled: Bool = true

        @ObservationIgnored var action: () -> Void = {}

        init(
            title: String,
            style: DSButtonStyle.Style,
            shape: DSButtonStyle.Shape,
            size: DSButtonStyle.Size,
            leadingIcon: ImageResource?,
            trailingIcon: ImageResource?,
            expands: Bool
        ) {
            self.title = title
            self.style = style
            self.shape = shape
            self.size = size
            self.leadingIcon = leadingIcon
            self.trailingIcon = trailingIcon
            self.expands = expands
        }
    }

    struct HostedButton: View {
        let model: Model

        var body: some View {
            DSButton(
                model.title,
                style: model.style,
                shape: model.shape,
                size: model.size,
                leadingIcon: model.leadingIcon,
                trailingIcon: model.trailingIcon,
                expands: model.expands,
                action: model.action
            )
            .disabled(!model.isEnabled)
        }
    }
}
