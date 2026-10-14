import SwiftUI
import DesignSystem

public struct AppPermissionsViewLayout: View {
    public struct Item: Identifiable, Hashable {
        public let id: String
        public let title: String
        public let description: String
        public var isOn: Bool

        public init(
            id: String,
            title: String,
            description: String,
            isOn: Bool
        ) {
            self.id = id
            self.title = title
            self.description = description
            self.isOn = isOn
        }
    }

    @State public var viewModel = AppPermissionsViewModel()

    public init() {}

    public var body: some View {
        if viewModel.items.isEmpty {
            emptyState
        } else {
            permissionsList
        }
    }
}

// MARK: - Private Views

private extension AppPermissionsViewLayout {
    var emptyState: some View {
        VStack {
            Spacer()
            Text(String(localized: .appPermissionsEmpty))
                .typography(.bodyMedium)
                .foregroundColor(Color(.fgSecondary))
            Spacer()
        }
    }

    var permissionsList: some View {
        List {
            ForEach(viewModel.items) { item in
                permissionRow(for: item)
            }
        }
        .listRowSpacing(16)
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    func permissionRow(for item: Item) -> some View {
        HStack(alignment: .top, spacing: 16) {
            HStack(alignment: .top, spacing: 8) {
                AppIconPlaceholder(size: 32, cornerRadius: 8)

                VStack(alignment: .leading, spacing: 8) {
                    Text(item.title)
                        .typography(.titleLarge)
                        .foregroundColor(Color(.fgPrimary))

                    Text(item.description)
                        .typography(.paragraphLarge)
                        .foregroundColor(Color(.fgSecondary))
                }
            }

            Spacer()

            Toggle(
                "",
                isOn: Binding(
                    get: { item.isOn },
                    set: { [item, viewModel] newValue in
                        viewModel.onToggle?(item, newValue)
                    }
                )
            )
            .labelsHidden()
            .tint(Color(.fgSuccess))
        }
        .padding(.vertical, 4)
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
    }
}
