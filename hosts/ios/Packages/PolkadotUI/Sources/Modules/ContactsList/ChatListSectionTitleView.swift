import DesignSystem
import SwiftUI
import UIKit

/// The title of a section of the chat list, for example the messages that a search found.
struct ChatListSectionTitleConfiguration: HashableContentConfiguration {
    let title: String

    func makeContentView() -> any UIView & UIContentView {
        UIHostingConfiguration {
            Text(title)
                .typography(.bodyMedium.emphasized)
                .foregroundStyle(Color.fgTertiary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, DSSpacings.extraMedium)
                .padding(.top, DSSpacings.extraMedium)
        }
        .margins(.all, 0)
        .makeContentView()
    }
}
