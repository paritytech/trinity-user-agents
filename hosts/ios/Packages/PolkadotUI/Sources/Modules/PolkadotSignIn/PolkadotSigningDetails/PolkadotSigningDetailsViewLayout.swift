import UIKit
import DesignSystem

public final class PolkadotSigningDetailsViewLayout: UIView {
    private let textView: UITextView = create {
        $0.backgroundColor = .bgSurfaceMain
        $0.isEditable = false
        $0.font = .app(.bodyMedium)
        $0.textColor = .fgSecondary
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

public extension PolkadotSigningDetailsViewLayout {
    struct ViewModel {
        public let text: String
        public let isTransaction: Bool

        public init(text: String, isTransaction: Bool) {
            self.text = text
            self.isTransaction = isTransaction
        }
    }

    func bind(viewModel: ViewModel) {
        textView.text = viewModel.text
    }
}

private extension PolkadotSigningDetailsViewLayout {
    func setupLayout() {
        backgroundColor = .bgSurfaceMain

        addSubview(textView)
        textView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }
}
