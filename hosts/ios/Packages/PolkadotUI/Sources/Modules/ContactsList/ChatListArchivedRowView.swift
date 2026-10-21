import DesignSystem
import SwiftUI
import UIKit

/// The row at the top of the chat list that opens the archived rooms.
public struct ChatListArchivedRowConfiguration: HashableContentConfiguration {
    let title: String
    let count: Int

    public init(title: String, count: Int) {
        self.title = title
        self.count = count
    }

    public func makeContentView() -> any UIView & UIContentView {
        UIHostingConfiguration {
            ChatListArchivedRow(title: title, count: count)
        }
        .margins(.all, 0)
        .makeContentView()
    }
}

struct ChatListArchivedRow: View {
    let title: String
    let count: Int

    var body: some View {
        HStack(spacing: DSSpacings.medium) {
            Image(systemName: "archivebox")
                .foregroundStyle(Color.fgSecondary)
                .frame(width: ChatListArchivedRowMetrics.iconWidth)
            Text(title)
                .typography(.titleMedium)
                .foregroundStyle(Color.fgPrimary)
            Spacer(minLength: 0)
            Text(String(count))
                .typography(.bodyMedium.emphasized)
                .foregroundStyle(Color.fgTertiary)
            Image(systemName: "chevron.right")
                .foregroundStyle(Color.fgTertiary)
        }
        .padding(DSSpacings.extraMedium)
        .contentShape(Rectangle())
    }
}

/// The icon column has the width of the avatar of a chat row, so the title aligns with the names of the chats.
private enum ChatListArchivedRowMetrics {
    static let iconWidth: CGFloat = 64
}
