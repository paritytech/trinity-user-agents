import ExternalAccessibility
import Foundation
import SwiftUI
import PolkadotUI
import Coinage

struct SettingsContentInput {
    let visibleCells: Set<SettingsViewModel.CellType>
    let attentionItems: Set<SettingsViewModel.CellType>
    let selectedCurrencyCode: String?
    let selectedThemeName: String?
    let selectedPrivacyStrategy: RecyclingStrategyType?
    let appVersion: String?
    let isTabBarLabelsEnabled: Bool
    let onSelect: (SettingsViewModel.CellType) -> Void
    let onSelectPrivacyStrategy: (RecyclingStrategyType) -> Void
    let onToggleTabBarLabels: (Bool) -> Void
}

protocol SettingsViewModelMaking {
    func makeContent(_ input: SettingsContentInput) -> SettingsViewModel.Content
}

final class SettingsViewModelFactory: SettingsViewModelMaking {
    func makeContent(_ input: SettingsContentInput) -> SettingsViewModel.Content {
        let sections: [SettingsViewLayout.Section] = SettingsViewModel.Section.allCases
            .compactMap { section -> SettingsViewLayout.Section? in
                let cells = section.cells.filter(input.visibleCells.contains)
                guard !cells.isEmpty else { return nil }
                return SettingsViewLayout.Section(
                    id: section.rawValue,
                    header: section.header,
                    leadingContent: privacyLeadingContent(for: section, input: input),
                    items: cells.map { cellType in
                        makeItem(cellType: cellType, input: input)
                    }
                )
            }
        return SettingsViewModel.Content(sections: sections, appVersion: input.appVersion)
    }
}

private extension SettingsViewModelFactory {
    func privacyLeadingContent(
        for section: SettingsViewModel.Section,
        input: SettingsContentInput
    ) -> AnyView? {
        guard section == .security, let selectedStrategy = input.selectedPrivacyStrategy else { return nil }
        return AnyView(
            PaymentPrivacyModeCard(selected: selectedStrategy, onSelect: input.onSelectPrivacyStrategy)
        )
    }

    func makeItem(
        cellType: SettingsViewModel.CellType,
        input: SettingsContentInput
    ) -> DSMenuListItem {
        let needsAttention = input.attentionItems.contains(cellType)
        let usesToggle = cellType == .tabBarLabels
        return DSMenuListItem(
            id: cellType,
            title: cellType.title,
            description: needsAttention ? cellType.attentionDetails?.message : nil,
            style: needsAttention ? .attention : .default,
            icon: icon(for: cellType),
            rightSlot: rightSlot(for: cellType, input: input),
            accessibilityId: AccessibilityID.Settings.menuItem(for: cellType),
            action: usesToggle ? nil : { input.onSelect(cellType) }
        )
    }

    func icon(for cellType: SettingsViewModel.CellType) -> ImageResource? {
        switch cellType {
        case .backup: .iconCloud
        case .theme: .iconPalette
        case .tabBarLabels: .iconPalette
        case .currency: .iconDollar
        case .linkedDevices: .iconLaptopMinimal
        case .apps: .iconGrid
        case .blockedUsers: .iconBlock
        case .legalSupport: .iconFile
        }
    }

    func rightSlot(
        for cellType: SettingsViewModel.CellType,
        input: SettingsContentInput
    ) -> DSMenuListItemRightSlot.Style? {
        switch cellType {
        case .theme:
            input.selectedThemeName.map(DSMenuListItemRightSlot.Style.labelChevron)
        case .tabBarLabels:
            .toggle(Binding(
                get: { input.isTabBarLabelsEnabled },
                set: input.onToggleTabBarLabels
            ))
        case .currency:
            if let selectedCurrencyCode = input.selectedCurrencyCode {
                .labelChevron(selectedCurrencyCode)
            } else {
                .chevron
            }
        case .backup,
             .linkedDevices,
             .apps,
             .blockedUsers,
             .legalSupport:
            .chevron
        }
    }
}
