import Foundation

extension Chat.LocalMessage.Content.Transfer {
    func withState(_ state: State?) -> Self {
        Self(totalValue: totalValue, coinKeys: coinKeys, state: state)
    }
}
