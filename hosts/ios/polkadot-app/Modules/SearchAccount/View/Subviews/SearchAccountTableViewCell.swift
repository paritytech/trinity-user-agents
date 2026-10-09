import SubstrateSdk
import UIKit
import SnapKit
import SwiftUI
import ExternalAccessibility
import PolkadotUI

final class SearchAccountTableViewCell: PlainBaseTableViewCell<SearchAccountContentView> {
    override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
        super.init(style: style, reuseIdentifier: reuseIdentifier)
    }

    // MARK: Public methods

    func bind(cellType: SearchAccountViewController.Cell) {
        contentDisplayView.bind(cellType: cellType)
    }
}

final class SearchAccountContentView: UIView {
    // MARK: Properties

    private let avatar = DSAvatarView(size: .s40)

    private let titleLabel: UILabel = .create {
        $0.font = .semibold16
        $0.textColor = .fgPrimary
        $0.lineBreakMode = .byTruncatingMiddle
    }

    // MARK: Initial methods

    override init(frame: CGRect) {
        super.init(frame: frame)

        configureConstraints()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Public methods

    fileprivate func bind(cellType: SearchAccountViewController.Cell) {
        guard let accountType = cellType.accountType else { return }

        titleLabel.text = accountType.title
        avatar.viewModel = accountType.avatarViewModel
    }

    // MARK: Private methods

    private func configureConstraints() {
        addSubview(avatar)
        addSubview(titleLabel)

        avatar.snp.makeConstraints {
            $0.centerY.equalToSuperview()
            $0.left.equalToSuperview().inset(16)
            $0.top.bottom.equalToSuperview().inset(8).priority(.high)
            $0.height.width.equalTo(avatar.proposedDimension)
        }

        titleLabel.snp.makeConstraints {
            $0.leading.equalTo(avatar.snp.trailing).offset(12)
            $0.top.equalTo(avatar.snp.top).inset(12).priority(.medium)
            $0.bottom.equalTo(avatar.snp.bottom).inset(12).priority(.medium)
            $0.centerY.equalToSuperview()
            $0.trailing.equalToSuperview().inset(16)
        }
    }
}

private extension SearchAccountViewModel.AccountType {
    var avatarViewModel: AvatarViewModel {
        .colored(
            text: String(title.prefix(1)),
            colorSeed: (try? accountAddress.toAccountId().toHex()) ?? accountAddress
        )
    }
}

// MARK: - AccessibilityBound

extension SearchAccountContentView: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [.init(self, AccessibilityID.SendPayment.searchResultRow)]
    }
}
