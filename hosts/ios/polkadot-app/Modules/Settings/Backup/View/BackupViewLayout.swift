import UIKit
import SnapKit
import DesignSystem

final class BackupViewLayout: UIView {
    // MARK: Properties

    weak var buttonsDelegate: BackupButtonsViewDelegate? {
        didSet {
            buttonsView.delegate = buttonsDelegate
        }
    }

    let restoredBalanceContainer = UIView()
    private let infoView = BackupInfoView()
    private let buttonsView = BackupButtonsView()
    private var restoredBalanceHeight: Constraint?

    // MARK: Initial methods

    override init(frame: CGRect) {
        super.init(frame: frame)

        configureView()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Public methods

    func bind(model: BackupViewModel) {
        infoView.bind(type: convertStatusToInfoType(model.statusType))
        buttonsView.bind(model: model.statusType)
    }

    func setRestoredBalanceVisible(_ isVisible: Bool) {
        restoredBalanceContainer.isHidden = !isVisible

        if isVisible {
            restoredBalanceHeight?.deactivate()
        } else {
            restoredBalanceHeight?.activate()
        }

        infoView.snp.updateConstraints {
            $0.top.equalTo(restoredBalanceContainer.snp.bottom)
                .offset(isVisible ? Constants.defaultOffset : Constants.infoViewTopOffset)
        }
    }

    // MARK: Private methods

    private func convertStatusToInfoType(
        _ statusType: BackupViewModel.BackupStatusType
    ) -> BackupInfoView.InfoType {
        switch statusType {
        case .created: .backup(.created)
        case .notFound: .backup(.notFound)
        case .cloudIsOff: .backup(.icloudIsOff)
        }
    }

    private func configureView() {
        addSubview(restoredBalanceContainer)
        addSubview(infoView)
        addSubview(buttonsView)

        restoredBalanceContainer.isHidden = true
        restoredBalanceContainer.snp.makeConstraints {
            $0.top.equalTo(safeAreaLayoutGuide.snp.top)
            $0.left.right.equalToSuperview().inset(Constants.cardOffset)
            restoredBalanceHeight = $0.height.equalTo(0).constraint
        }

        infoView.snp.makeConstraints {
            $0.top.equalTo(restoredBalanceContainer.snp.bottom).offset(Constants.infoViewTopOffset)
            $0.left.right.equalToSuperview().inset(Constants.defaultOffset)
        }

        buttonsView.snp.makeConstraints {
            $0.left.right.equalToSuperview().inset(Constants.defaultOffset)
            $0.bottom.equalTo(safeAreaLayoutGuide.snp.bottom).inset(16)
        }
    }
}

// MARK: - Constants

private enum Constants {
    static let infoViewTopOffset: CGFloat = 85
    static let defaultOffset: CGFloat = 24
    static let cardOffset = DSSpacings.mediumIncreased
}
