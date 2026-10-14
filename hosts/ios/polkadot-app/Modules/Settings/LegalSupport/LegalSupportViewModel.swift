import Foundation
import PolkadotUI

enum LegalSupportViewModel {
    static func makeSections(onSelect: @escaping (CellType) -> Void) -> [SettingsViewLayout.Section] {
        Section.allCases.map { section in
            SettingsViewLayout.Section(
                id: section.rawValue,
                header: section.header,
                items: section.cells.map { cell in
                    DSMenuListItem(
                        id: cell,
                        title: cell.title,
                        rightSlot: .chevron,
                        action: { onSelect(cell) }
                    )
                }
            )
        }
    }

    enum CellType: Hashable {
        case privacy
        case termsOfUse
        case contactUs

        var title: String {
            switch self {
            case .privacy: String(localized: .settingsCellPrivacy)
            case .termsOfUse: String(localized: .settingsCellTerms)
            case .contactUs: String(localized: .settingsCellContactUs)
            }
        }
    }

    enum Section: String, CaseIterable {
        case legal
        case support

        var header: String {
            switch self {
            case .legal: String(localized: .settingsSectionLegal)
            case .support: String(localized: .settingsSectionSupport)
            }
        }

        var cells: [CellType] {
            switch self {
            case .legal: [.privacy, .termsOfUse]
            case .support: [.contactUs]
            }
        }
    }
}
