import UIKit

// TODO: consider refactoring

struct WalletCardCreateViewModel {
    let info: TokenCardInfo
    let style: Style

    struct CardStyle {
        let backgroundColor: UIColor
    }

    struct Style {
        let background: CardStyle
    }

    struct TokenCardInfo {
        let name: String
    }
}

enum WalletCardDataViewModel {
    case token(BalanceViewModelProtocol)
}
