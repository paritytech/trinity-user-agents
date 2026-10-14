import SwiftUI
import DesignSystem

public struct BlockedUsersViewLayout: View {
    public struct Item: Identifiable {
        public let id: String
        public let username: String

        public init(id: String, username: String) {
            self.id = id
            self.username = username
        }
    }

    @State public var viewModel = BlockedUsersViewModel()

    public init() {}

    public var body: some View {
        if viewModel.items.isEmpty {
            emptyState
        } else {
            blockedUsersList
        }
    }
}

// MARK: - Private Views

private extension BlockedUsersViewLayout {
    var emptyState: some View {
        VStack {
            Spacer()
            Text(String(localized: .blockedUsersEmpty))
                .typography(.bodyMedium)
                .foregroundStyle(Color.fgSecondary)
            Spacer()
        }
    }

    var blockedUsersList: some View {
        List {
            ForEach(viewModel.items) { user in
                blockedUserRow(for: user)
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    func blockedUserRow(for user: Item) -> some View {
        HStack {
            Text(user.username)
                .typography(.titleLarge)
                .foregroundStyle(Color.fgPrimary)

            Spacer()

            Button {
                viewModel.onUnblock?(user)
            } label: {
                Text(String(localized: .blockedUsersUnblock))
                    .typography(.bodyMedium)
                    .foregroundStyle(Color.fgSecondary)
            }
            .buttonStyle(.plain)
        }
        .frame(minHeight: 48)
        .contentShape(Rectangle())
        .onTapGesture {
            viewModel.onSelect?(user)
        }
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
    }
}
