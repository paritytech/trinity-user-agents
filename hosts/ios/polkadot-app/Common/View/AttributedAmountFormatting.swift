import UIKit

protocol AttributedBalanceFormatting {
    func getFormattedString(from balance: BalanceViewModelProtocol) -> NSAttributedString
}

final class InlineBalanceFormatter: AttributedBalanceFormatting {
    func getFormattedString(from viewModel: BalanceViewModelProtocol) -> NSAttributedString {
        guard let price = viewModel.price else {
            return NSAttributedString(
                string: viewModel.amount,
                attributes: [.foregroundColor: UIColor.fgPrimary]
            )
        }
        let totalString = NSMutableAttributedString(
            string: price,
            attributes: [.foregroundColor: UIColor.fgPrimary]
        )

        let amountString = NSAttributedString(
            string: " · " + viewModel.amount,
            attributes: [.foregroundColor: UIColor.fgTertiary]
        )

        totalString.append(amountString)

        return totalString
    }
}
