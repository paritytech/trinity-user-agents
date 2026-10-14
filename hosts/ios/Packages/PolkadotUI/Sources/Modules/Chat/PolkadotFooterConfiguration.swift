import Foundation
import UIKit

public enum PolkadotFooterConfiguration {
    public static func footer(
        messages: [UIAction],
        title: String,
        actions: [ChatMessageActionView.ViewModel]
    ) -> any HashableContentConfiguration {
        let bgConfig = ChatSystemMessageConfiguration.BackgroundConfiguration(
            color: .bgSurfaceContainer,
            cornerRadius: 24,
            insets: .all(insets: 12)
        )
        let actions = actions.map {
            let view = ChatMessageActionView(viewModel: $0)
            return SwiftUIContentConfiguration(view: view)
        }.map {
            ChatSystemMessageConfiguration(
                contentProvider: $0,
                textBackgroundConfiguration: bgConfig,
                contentInsets: .zero
            )
        }

        let faqConfig = messages.isEmpty ? nil : FAQViewConfiguration(actions: messages)

        let actionsConfig = PolkadotFooterActionsConfiguration(
            title: title,
            actions: actions
        )

        return GenericFooterConfiguration(
            faqConfiguration: faqConfig,
            contentConfiguration: actionsConfig
        )
    }
}
