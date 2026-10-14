import UIKit
internal import SnapKit

// Any view capable of display activity indication eg: UIActivityIndicator, some Lotty view, etc.
public protocol LoadIndicatorRepresentable: UIView {
    func startAnimating()
    func stopAnimating()
    var isAnimating: Bool { get }
}

open class GenericLoadableView<Content: UIView, Indicator: LoadIndicatorRepresentable>: UIView {
    public let contentView: Content
    public let indicatorView: Indicator

    open var loadingOverlayAnchor: UIView {
        contentView
    }

    public private(set) var isLoading: Bool = false

    public init(contentView: Content = Content(), indicatorView: Indicator = Indicator()) {
        self.contentView = contentView
        self.indicatorView = indicatorView
        super.init(frame: .zero)
        setup()
    }

    override public init(frame: CGRect) {
        contentView = Content()
        indicatorView = Indicator()
        super.init(frame: frame)
        setup()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setup() {
        addSubview(contentView)
        addSubview(indicatorView)

        contentView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
        indicatorView.snp.makeConstraints {
            $0.center.equalTo(loadingOverlayAnchor.snp.center)
        }
    }

    open func startLoading() {
        guard !isLoading else { return }
        isLoading = true

        UIView.transition(
            with: self,
            duration: 0.3,
            options: [.transitionCrossDissolve, .allowUserInteraction]
        ) { [self] in
            indicatorView.startAnimating()
            indicatorView.alpha = 1
            indicatorView.transform = .identity

            loadingOverlayAnchor.layer.opacity = 0
            loadingOverlayAnchor.transform = .identity.scaledBy(x: 0.9, y: 0.9)
        }
    }

    open func stopLoading() {
        guard isLoading else { return }
        isLoading = false

        UIView.transition(
            with: self,
            duration: 0.3,
            options: [.transitionCrossDissolve, .allowUserInteraction]
        ) { [self] in
            indicatorView.alpha = 0
            indicatorView.transform = .identity.scaledBy(x: 0.9, y: 0.9)

            loadingOverlayAnchor.layer.opacity = 1
            loadingOverlayAnchor.transform = .identity
        } completion: { [self] _ in
            indicatorView.stopAnimating()
        }
    }
}
