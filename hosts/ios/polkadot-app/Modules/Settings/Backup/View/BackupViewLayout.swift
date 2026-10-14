import UIKit
import SnapKit

final class BackupViewLayout: UIView {
    // MARK: Properties

    weak var buttonsDelegate: BackupButtonsViewDelegate? {
        didSet {
            buttonsView.delegate = buttonsDelegate
        }
    }

    private let infoView = BackupInfoView()
    private let buttonsView = BackupButtonsView()

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
        addSubview(infoView)
        addSubview(buttonsView)

        infoView.snp.makeConstraints {
            $0.top.equalTo(safeAreaLayoutGuide.snp.top).inset(Constants.infoViewTopOffset)
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
}
