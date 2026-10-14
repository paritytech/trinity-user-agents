import UIKit
import ExternalAccessibility
import PolkadotUI
import UIKit_iOS
import SnapKit

final class SearchAccountViewLayout: UIView {
    // MARK: Properties

    let tableView: UITableView = .create {
        $0.keyboardDismissMode = .onDrag
        $0.separatorStyle = .none
    }

    let scanButton = UIBarButtonItem(
        image: .scanBarButton,
        style: .plain,
        target: nil,
        action: nil
    )
    let addressInputView = RecipientInputView()

    let loadingView: LoadingView = .create {
        $0.contentBackgroundColor = .clear
        $0.contentSize = CGSize(width: 48, height: 48)
        $0.indicatorImage = .loadingIndicator
        $0.isHidden = true
    }

    // MARK: Initial methods

    override init(frame: CGRect) {
        super.init(frame: frame)

        tableView.backgroundColor = .clear

        backgroundColor = .bgSurfaceMain
        configureView()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Private methods

    private func configureView() {
        addSubview(addressInputView)
        addSubview(tableView)
        addSubview(loadingView)

        addressInputView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview()
            $0.top.equalTo(safeAreaLayoutGuide.snp.top)
            $0.height.equalTo(56)
        }

        tableView.snp.makeConstraints {
            $0.top.equalTo(addressInputView.snp.bottom)
            $0.left.right.bottom.equalToSuperview()
        }

        loadingView.snp.makeConstraints {
            $0.center.equalToSuperview()
        }
    }
}

// MARK: - AccessibilityBound

extension SearchAccountViewLayout: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [.init(addressInputView.textField, AccessibilityID.SendPayment.recipientInput)]
    }
}
