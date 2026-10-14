import DesignSystem
import SwiftUI

public struct SettingsViewLayout: View {
    private let sections: [Section]
    private let appVersion: String?

    @Environment(\.appTheme) private var appTheme

    public init(sections: [Section], appVersion: String?) {
        self.sections = sections
        self.appVersion = appVersion
    }

    public var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: DSSpacings.large) {
                ForEach(sections) { section in
                    sectionView(section)
                }
                if let appVersion {
                    appVersionFooter(version: appVersion)
                }
                Spacer(minLength: DSSpacings.extraLargeIncreased)
            }
            .padding(.horizontal, DSSpacings.mediumIncreased)
            .padding(.top, DSSpacings.large)
        }
        .scrollIndicators(.hidden)
        .environment(\.appTheme, appTheme)
    }
}

public extension SettingsViewLayout {
    struct Section: Identifiable {
        public let id: String
        public let header: String?
        public let leadingContent: AnyView?
        public let items: [DSMenuListItem]

        public init(id: String, header: String?, leadingContent: AnyView? = nil, items: [DSMenuListItem]) {
            self.id = id
            self.header = header
            self.leadingContent = leadingContent
            self.items = items
        }
    }
}

private extension SettingsViewLayout {
    func sectionView(_ section: Section) -> some View {
        VStack(alignment: .leading, spacing: DSSpacings.extraMedium) {
            if let header = section.header {
                DSCaption(header)
            }
            groupedItems(section)
        }
    }

    func groupedItems(_ section: Section) -> some View {
        let hasLeading = section.leadingContent != nil
        let count = section.items.count
        return VStack(spacing: 0) {
            if let leadingContent = section.leadingContent {
                leadingContent
                    .dsMenuListCellSurface(
                        position: count == 0 ? .standalone : .first,
                        showsDivider: count > 0
                    )
            }
            ForEach(Array(section.items.enumerated()), id: \.element.id) { index, item in
                item.dsMenuListItemPosition(position(at: index, count: count, hasLeading: hasLeading))
            }
        }
    }

    func position(at index: Int, count: Int, hasLeading: Bool) -> DSMenuListItemPosition {
        if hasLeading {
            return index == count - 1 ? .last : .middle
        }
        switch (index, count) {
        case (_, 1): return .standalone
        case (0, _): return .first
        case let (last, count) where last == count - 1: return .last
        default: return .middle
        }
    }

    func appVersionFooter(version: String) -> some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            Image(.logoPolkadot)
                .resizable()
                .scaledToFit()
                .frame(height: 28)
                .foregroundStyle(Color.fgPrimary)

            Text(version)
                .typography(.bodyMedium)
                .foregroundStyle(Color.fgTertiary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, DSSpacings.small)
    }
}

#if DEBUG
    #Preview("SettingsViewLayout") {
        SettingsViewLayout(
            sections: [
                SettingsViewLayout.Section(
                    id: "general",
                    header: "General",
                    items: [
                        DSMenuListItem(
                            id: "theme",
                            title: "Theme",
                            icon: .iconAlertCircle,
                            rightSlot: .labelChevron("Jam"),
                            action: {}
                        )
                    ]
                ),
                SettingsViewLayout.Section(
                    id: "security",
                    header: "Security & privacy",
                    items: [
                        DSMenuListItem(
                            id: "backup",
                            title: "Backup",
                            description: "Not backed up. Set up a backup to stay protected",
                            style: .attention,
                            icon: .iconAlertCircle,
                            rightSlot: .chevron,
                            action: {}
                        ),
                        DSMenuListItem(
                            id: "apps",
                            title: "Apps",
                            icon: .iconAlertCircle,
                            rightSlot: .chevron,
                            action: {}
                        ),
                        DSMenuListItem(
                            id: "blocked",
                            title: "Blocked contacts",
                            icon: .iconAlertCircle,
                            rightSlot: .chevron,
                            action: {}
                        )
                    ]
                )
            ],
            appVersion: "App Version v1.0.0 (2026)"
        )
        .background(Color.bgSurfaceMain)
    }
#endif
