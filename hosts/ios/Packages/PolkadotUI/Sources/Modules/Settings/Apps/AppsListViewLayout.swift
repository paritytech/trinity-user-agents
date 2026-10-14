import SwiftUI
import DesignSystem

public struct AppsListViewLayout: View {
    public struct Item: Identifiable {
        public let id: String
        public let name: String
        /// The product's domain, shown only when the manifest gave it a different display name.
        public let subtitle: String?
        /// Shown until ``icon`` loads, and for products that never resolve one.
        public let avatar: AvatarViewModel
        public let icon: (any ImageViewModelProtocol)?

        public init(
            id: String,
            name: String,
            subtitle: String?,
            avatar: AvatarViewModel,
            icon: (any ImageViewModelProtocol)?
        ) {
            self.id = id
            self.name = name
            self.subtitle = subtitle
            self.avatar = avatar
            self.icon = icon
        }
    }

    @State public var viewModel = AppsListViewModel()

    public init() {}

    public var body: some View {
        if viewModel.items.isEmpty {
            emptyState
        } else {
            appsList
        }
    }
}

// MARK: - Private Views

private extension AppsListViewLayout {
    var emptyState: some View {
        VStack {
            Spacer()
            Text(String(localized: .appsListEmpty))
                .typography(.bodyMedium)
                .foregroundColor(Color(.fgSecondary))
            Spacer()
        }
    }

    var appsList: some View {
        List {
            ForEach(viewModel.items) { item in
                appRow(for: item)
            }
        }
        .listRowSpacing(16)
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    func appRow(for item: Item) -> some View {
        DisclosureListRow(
            title: item.name,
            subtitle: item.subtitle,
            leading: { DSAsyncAvatar(placeholder: item.avatar, icon: item.icon, size: .s40) },
            onTap: { viewModel.onSelect?(item) }
        )
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
    }
}
