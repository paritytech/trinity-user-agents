import UIKit
import DesignSystem
import ExternalAccessibility
internal import UIKit_iOS

public final class PolkadotSigningViewLayout: BottomSheetOperationViewLayout<
    PolkadotSigningResultView,
    PolkadotSigningResultView.ViewModel
> {
    override public func bind(resultViewModel: PolkadotSigningResultView.ViewModel) {
        resultView.bind(viewModel: resultViewModel)
    }
}

public final class PolkadotSigningResultView: UIView {
    private let imageView: UIImageView = create {
        $0.contentMode = .scaleAspectFit
    }

    private let labelsView: TopBottomLabelView = create {
        $0.spacing = 12

        $0.topLabel.typography = .headlineSmall
        $0.topLabel.textColor = .fgPrimary
        $0.topLabel.numberOfLines = 2
        $0.topLabel.textAlignment = .center
        $0.topLabel.text = " \n "

        $0.bottomLabel.typography = .paragraphLarge
        $0.bottomLabel.textColor = .fgTertiary
        $0.bottomLabel.numberOfLines = 1
        $0.bottomLabel.textAlignment = .center
        $0.bottomLabel.text = " "
    }

    private let _viewDetailsButton: RoundedButton = create {
        $0.applySecondaryStyle()
        $0.setTitle(.init(localized: .actionViewDetails))
    }

    private let buttonsView: GenericPairValueView<RoundedButton, RoundedButton> = create {
        $0.spacing = 12
        $0.makeHorizontal()

        $0.fView.applySecondaryStyle()
        $0.fView.setTitle(.init(localized: .actionReject))

        $0.sView.applyMainStyle()
        $0.sView.setTitle(.init(localized: .actionSign))
    }

    public var signButton: UIControl {
        buttonsView.sView
    }

    public var rejectButton: UIControl {
        buttonsView.fView
    }

    public var viewDetailsButton: UIControl {
        _viewDetailsButton
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension PolkadotSigningResultView {
    func setupLayout() {
        addSubview(imageView)
        imageView.snp.makeConstraints {
            $0.top.equalToSuperview()
            $0.width.height.equalTo(48)
            $0.centerX.equalToSuperview()
        }

        addSubview(labelsView)
        labelsView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(8)
            $0.top.equalTo(imageView.snp.bottom).offset(16)
        }

        addSubview(_viewDetailsButton)
        _viewDetailsButton.snp.makeConstraints {
            $0.height.equalTo(UIConstants.actionHeight)
            $0.leading.trailing.equalToSuperview()
            $0.top.equalTo(labelsView.snp.bottom).offset(24)
        }

        addSubview(buttonsView)
        buttonsView.snp.makeConstraints {
            $0.leading.trailing.bottom.equalToSuperview()
            $0.top.equalTo(_viewDetailsButton.snp.bottom).offset(16)
        }

        signButton.snp.makeConstraints {
            $0.height.equalTo(UIConstants.actionHeight)
            $0.width.equalTo(rejectButton.snp.width)
        }
    }
}

public extension PolkadotSigningResultView {
    struct ViewModel {
        let hostName: String
        let iconViewModel: ImageViewModelProtocol?
        let transactionDescription: String

        public init(
            hostName: String,
            iconViewModel: ImageViewModelProtocol?,
            transactionDescription: String
        ) {
            self.hostName = hostName
            self.iconViewModel = iconViewModel
            self.transactionDescription = transactionDescription
        }
    }

    func bind(viewModel: ViewModel) {
        viewModel.iconViewModel?.loadImage(
            on: imageView,
            settings: .originalImage,
            animated: true,
            completion: nil
        )

        labelsView.topLabel.text = .init(
            localized: .polkadotSigningTitle(name: viewModel.hostName)
        )

        labelsView.bottomLabel.text = viewModel.transactionDescription
    }
}

// MARK: - AccessibilityBound

extension PolkadotSigningResultView: AccessibilityBound {
    public var accessibilityBindings: [AccessibilityBinding] {
        [
            .init(labelsView.topLabel, AccessibilityID.Signing.requestTitle),
            .init(signButton, AccessibilityID.Signing.approveButton),
            .init(rejectButton, AccessibilityID.Signing.rejectButton)
        ]
    }
}
