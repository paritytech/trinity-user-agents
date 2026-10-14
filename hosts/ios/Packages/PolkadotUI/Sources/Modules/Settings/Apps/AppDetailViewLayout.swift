import SwiftUI
import DesignSystem

public struct AppDetailViewLayout: View {
    @State public var viewModel = AppDetailViewModel()

    public init() {}

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 48) {
                header
                privacySection
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 16)
        }
        .scrollIndicators(.hidden)
    }
}

// MARK: - Private Views

private extension AppDetailViewLayout {
    var header: some View {
        VStack(spacing: 8) {
            DSAsyncAvatar(placeholder: viewModel.avatar, icon: viewModel.icon, size: .s72)

            Text(viewModel.name)
                .typography(.headlineSmall)
                .foregroundColor(Color.fgPrimary)
                .multilineTextAlignment(.center)

            if let subtitle = viewModel.subtitle {
                Text(subtitle)
                    .typography(.bodyMedium)
                    .foregroundColor(Color.fgSecondary)
                    .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity)
    }

    var privacySection: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(String(localized: .appDetailPrivacySectionTitle))
                .typography(.paragraphSmall)
                .foregroundColor(Color.fgTertiary)
                .textCase(.uppercase)

            permissionsRow
        }
    }

    var permissionsRow: some View {
        DisclosureListRow(title: String(localized: .appDetailPermissionsCell)) {
            viewModel.onPermissionsTap?()
        }
    }
}
