import UIKit
import PolkadotUI
import DesignSystem

final class AccountPillView: GenericBorderedView<GenericPairValueView<AccountTypeView, Label>> {
    var accountTypeView: AccountTypeView {
        contentView.fView
    }

    var accountLabel: Label {
        contentView.sView
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        configure()
    }

    func bind(account: SearchAccountViewModel.AccountType) {
        accountTypeView.bind(account: account)
        switch account {
        case let .username(username, _):
            accountLabel.lineBreakMode = .byTruncatingTail
            accountLabel.numberOfLines = 0
            accountLabel.text = username

        case let .accountAddress(accountAddress):
            accountLabel.lineBreakMode = .byTruncatingMiddle
            accountLabel.numberOfLines = 1
            accountLabel.text = accountAddress
        }
    }

    private func configure() {
        contentView.setHorizontalAndSpacing(8)
        contentView.stackView.alignment = .center
        contentInsets = UIEdgeInsets(top: 4, left: 4, bottom: 4, right: 12)
        backgroundView.applyBorderStyle(.fill6, cornerRadius: 16)

        accountTypeView.preferredSize = 24
        accountLabel.typography = .titleMedium
        accountLabel.textColor = .fgPrimary

        accountTypeView.setContentCompressionResistancePriority(.required, for: .horizontal)
        accountTypeView.setContentHuggingPriority(.required, for: .horizontal)
        accountTypeView.setContentHuggingPriority(.required, for: .vertical)
    }
}
