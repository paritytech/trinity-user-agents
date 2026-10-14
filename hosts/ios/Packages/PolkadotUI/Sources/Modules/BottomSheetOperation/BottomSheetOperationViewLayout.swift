import UIKit
import DesignSystem
internal import UIKit_iOS

open class BottomSheetOperationViewLayout<
    ResultView: UIView,
    ResultViewModel
>: BottomSheetBaseLayout {
    private let indicatorView = UIActivityIndicatorView(style: .medium)
    private let failureView = FailureView()

    public let resultView = ResultView()

    public var failureCloseButton: UIControl {
        failureView.closeButton
    }

    override func setupLayout() {
        super.setupLayout()

        indicatorView.color = .fgPrimary
        indicatorView.setHidden(true)
        failureView.setHidden(true)
        resultView.setHidden(true)

        contentView.addSubview(indicatorView)
        indicatorView.snp.makeConstraints {
            $0.centerX.centerY.equalToSuperview()
        }

        contentView.addSubview(resultView)
        resultView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        contentView.addSubview(failureView)
        failureView.snp.makeConstraints {
            $0.edges.equalTo(contentView.snp.edges)
        }
    }

    open func bind(resultViewModel _: ResultViewModel) {
        fatalError("to be overriden")
    }
}

public extension BottomSheetOperationViewLayout {
    enum ViewModel {
        case inProgress
        case failure(String)
        case result(ResultViewModel)
    }

    func bind(viewModel: ViewModel) {
        switch viewModel {
        case .inProgress:
            bindInProgress()
        case let .failure(string):
            bindFailureText(string)
        case let .result(viewModel):
            bindResult(viewModel)
        }
    }
}

private extension BottomSheetOperationViewLayout {
    func bindInProgress() {
        resultView.setHidden(true)
        failureView.setHidden(true)
        indicatorView.setHidden(false)
        indicatorView.startAnimating()
    }

    func bindFailureText(_ text: String) {
        resultView.setHidden(true)
        indicatorView.setHidden(true)
        indicatorView.stopAnimating()
        failureView.textLabel.text = text
        failureView.setHidden(false)
    }

    func bindResult(_ viewModel: ResultViewModel) {
        indicatorView.setHidden(true)
        indicatorView.stopAnimating()
        failureView.setHidden(true)
        resultView.setHidden(false)
        bind(resultViewModel: viewModel)
    }
}

private extension BottomSheetOperationViewLayout {
    final class FailureView: UIView {
        let textLabel: Label = create {
            $0.typography = .titleMedium
            $0.textColor = .fgPrimary
            $0.numberOfLines = 0
            $0.textAlignment = .center
        }

        let closeButton: RoundedButton = create {
            $0.applySecondaryStyle()
            $0.setTitle(.init(localized: .actionClose))
        }

        override init(frame: CGRect) {
            super.init(frame: frame)
            setupLayout()
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) has not been implemented")
        }
    }
}

private extension BottomSheetOperationViewLayout.FailureView {
    func setupLayout() {
        let textContentView = UIView()
        addSubview(textContentView)
        textContentView.snp.makeConstraints {
            $0.top.leading.trailing.equalToSuperview()
        }

        addSubview(closeButton)
        closeButton.snp.makeConstraints {
            $0.leading.trailing.bottom.equalToSuperview()
            $0.top.equalTo(textContentView.snp.bottom)
            $0.height.equalTo(UIConstants.actionHeight)
        }

        textContentView.addSubview(textLabel)
        textLabel.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(8)
            $0.centerY.equalToSuperview()
        }
    }
}
