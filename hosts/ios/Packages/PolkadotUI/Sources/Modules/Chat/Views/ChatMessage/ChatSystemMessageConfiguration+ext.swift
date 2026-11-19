import Foundation
import UIKit.UIColor
import SwiftUI

public extension ChatSystemMessageConfiguration {
    // TODO: Discuss why do we need to use ChatSystemMessage insted of any Content provider
    static func anyView(_ view: some View & Hashable) -> Self {
        ChatSystemMessageConfiguration(
            contentProvider: SwiftUIContentConfiguration(view: view)
        )
    }

    static func deposit(
        amount: String
    ) -> Self {
        let text = String(localized: .chatDepositAdded(amount: amount))
        return ChatSystemMessageConfiguration.text(.text(text))
    }

    static func text(_ viewModel: ChatSystemMessageTextView.ViewModel) -> Self {
        let view = ChatSystemMessageTextView(viewModel: viewModel)
        let configuration = SwiftUIContentConfiguration(view: view)

        let bgConfig = ChatSystemMessageConfiguration.BackgroundConfiguration(
            color: .clear,
            cornerRadius: 0,
            insets: .all(insets: 0)
        )

        return ChatSystemMessageConfiguration(
            contentProvider: configuration,
            textBackgroundConfiguration: bgConfig,
            contentInsets: .init(horizontal: 0, vertical: 12)
        )
    }
}
