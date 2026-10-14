import UIKit
public import UIKit_iOS

public final class LoadableActionView: UIView {
    public let actionLoadingView: ActionLoadingView = {
        let view = ActionLoadingView()
        view.isHidden = true
        return view
    }()

    public let actionButton: RoundedButton = {
        let button = RoundedButton()
        button.applyBaseFillStyle()
        return button
    }()

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupLayout() {
        addSubview(actionLoadingView)
        actionLoadingView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addSubview(actionButton)
        actionButton.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    public func startLoading() {
        actionButton.isHidden = true
        actionLoadingView.isHidden = false
        actionLoadingView.start()
    }

    public func stopLoading() {
        actionLoadingView.stop()
        actionButton.isHidden = false
        actionLoadingView.isHidden = true
    }

    public var isLoading: Bool {
        !actionLoadingView.isHidden
    }

    public func updateAnimation() {
        if actionLoadingView.isAnimating {
            actionLoadingView.stop()
            actionLoadingView.start()
        }
    }
}
