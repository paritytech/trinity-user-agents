import SwiftUI
import UIKit
internal import SnapKit

// UIKit wrapper that hosts the SwiftUI DSIconButtonStyle so UIKit call sites
// share a single source of truth for icon-button styling.
public final class DSIconButton: UIView {
    public var onTap: (() -> Void)?

    private let buttonSize: DSIconButtonStyle.Size
    private var hostingController: UIHostingController<DSIconButtonHostView>!

    public init(
        style: DSIconButtonStyle.Style,
        shape: DSIconButtonStyle.Shape,
        size: DSIconButtonStyle.Size,
        icon: UIImage,
        glass: Bool = false
    ) {
        buttonSize = size
        super.init(frame: .zero)
        backgroundColor = .clear

        let host = DSIconButtonHostView(
            style: style,
            shape: shape,
            size: size,
            glass: glass,
            icon: icon,
            action: { [weak self] in self?.onTap?() }
        )
        let controller = UIHostingController(rootView: host)
        controller.view.backgroundColor = .clear
        controller.sizingOptions = []
        // Fixed-size control — don't inset hosted content by window safe-area
        controller.safeAreaRegions = []

        hostingController = controller

        addSubview(controller.view)
        controller.view.snp.makeConstraints { $0.edges.equalToSuperview() }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public var intrinsicContentSize: CGSize {
        CGSize(width: buttonSize.dimension, height: buttonSize.dimension)
    }
}

private struct DSIconButtonHostView: View {
    let style: DSIconButtonStyle.Style
    let shape: DSIconButtonStyle.Shape
    let size: DSIconButtonStyle.Size
    let glass: Bool
    let icon: UIImage
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(uiImage: icon)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
        }
        .buttonStyle(.dsIcon(style: style, shape: shape, size: size, glass: glass))
    }
}
