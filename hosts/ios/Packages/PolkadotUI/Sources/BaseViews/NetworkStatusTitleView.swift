import UIKit
import DesignSystem
internal import SnapKit

public final class NetworkStatusTitleView: UIView {
    public struct ViewModel: Equatable {
        public let text: String
        public let isLoading: Bool

        public init(text: String, isLoading: Bool) {
            self.text = text
            self.isLoading = isLoading
        }
    }

    private static let indicatorDiameter: CGFloat = 22
    private static let indicatorTextSpacing: CGFloat = 8
    private static let rotationDuration: TimeInterval = 0.7
    private static let rotationAnimationKey = "networkStatusTitle.rotation"

    private let indicatorImageView: UIImageView = .create {
        $0.contentMode = .scaleAspectFit
        $0.image = UIImage(resource: .loading).withRenderingMode(.alwaysTemplate)
        $0.tintColor = .fgPrimary
    }

    private let titleLabel: Label = .create {
        $0.typography = .headlineSmall
        $0.textColor = .fgPrimary
    }

    private var viewModel: ViewModel?

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleDidBecomeActive),
            name: UIApplication.didBecomeActiveNotification,
            object: nil
        )
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func didMoveToWindow() {
        super.didMoveToWindow()

        guard window != nil, viewModel?.isLoading == true else {
            return
        }

        startRotation()
    }

    public func bind(viewModel: ViewModel) {
        guard viewModel != self.viewModel else {
            return
        }

        self.viewModel = viewModel

        titleLabel.typography = viewModel.isLoading ? .titleMedium : .headlineSmall
        titleLabel.text = viewModel.text
        indicatorImageView.isHidden = !viewModel.isLoading

        if viewModel.isLoading {
            startRotation()
        } else {
            stopRotation()
        }
    }
}

#if DEBUG
    #Preview("NetworkStatusTitleView states") {
        let states: [NetworkStatusTitleView.ViewModel] = [
            .init(text: "Chats", isLoading: false),
            .init(text: "Connecting...", isLoading: true),
            .init(text: "Waiting for network", isLoading: true)
        ]

        let stackView = UIStackView(
            arrangedSubviews: states.map { state in
                let view = NetworkStatusTitleView()
                view.bind(viewModel: state)
                return view
            }
        )
        stackView.axis = .vertical
        stackView.alignment = .leading
        stackView.spacing = 16

        let container = UIView()
        container.backgroundColor = .bgSurfaceMain
        container.addSubview(stackView)
        stackView.snp.makeConstraints {
            $0.center.equalToSuperview()
        }

        return container
    }
#endif

private extension NetworkStatusTitleView {
    @objc func handleDidBecomeActive() {
        guard window != nil, viewModel?.isLoading == true else {
            return
        }

        startRotation()
    }

    func setupLayout() {
        indicatorImageView.snp.makeConstraints {
            $0.size.equalTo(Self.indicatorDiameter)
        }

        let stackView = UIStackView(arrangedSubviews: [indicatorImageView, titleLabel])
        stackView.axis = .horizontal
        stackView.alignment = .center
        stackView.spacing = Self.indicatorTextSpacing

        addSubview(stackView)
        stackView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }
    }

    func startRotation() {
        guard indicatorImageView.layer.animation(forKey: Self.rotationAnimationKey) == nil else {
            return
        }

        let animation = CABasicAnimation(keyPath: "transform.rotation.z")
        animation.toValue = CGFloat.pi * 2
        animation.duration = Self.rotationDuration
        animation.repeatCount = .infinity
        animation.isRemovedOnCompletion = false

        indicatorImageView.layer.add(animation, forKey: Self.rotationAnimationKey)
    }

    func stopRotation() {
        indicatorImageView.layer.removeAnimation(forKey: Self.rotationAnimationKey)
    }
}
