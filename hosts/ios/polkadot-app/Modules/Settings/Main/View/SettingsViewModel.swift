import UIKit
import PolkadotUI

enum SettingsViewModel {
    struct AttentionDetails {
        let message: String
        let imageResource: ImageResource
    }

    enum CellType: Hashable {
        case backup
        case theme
        case tabBarLabels
        case currency
        case linkedDevices
        case apps
        case blockedUsers
        case legalSupport

        var title: String {
            switch self {
            case .backup: String(localized: .settingsCellBackup)
            case .theme: String(localized: .settingsCellTheme)
            case .tabBarLabels: String(localized: .settingsCellTabBarLabels)
            case .currency: String(localized: .settingsCellCurrency)
            case .linkedDevices: String(localized: .settingsCellLinkedDevices)
            case .apps: String(localized: .settingsCellApps)
            case .blockedUsers: String(localized: .settingsCellBlockedUsers)
            case .legalSupport: String(localized: .settingsCellLegalSupport)
            }
        }

        var attentionDetails: AttentionDetails? {
            switch self {
            case .backup:
                AttentionDetails(
                    message: String(localized: .settingsCellBackupAttention),
                    imageResource: .error
                )
            default:
                nil
            }
        }
    }

    enum Section: String, CaseIterable {
        case general
        case security
        case legalSupport

        var header: String {
            switch self {
            case .general: String(localized: .settingsSectionGeneral)
            case .security: String(localized: .settingsSectionSecurity)
            case .legalSupport: String(localized: .settingsSectionLegalSupport)
            }
        }

        var cells: [CellType] {
            switch self {
            case .general: [.theme, .tabBarLabels]
            case .security: Self.securityCells
            case .legalSupport: [.legalSupport]
            }
        }

        private static var securityCells: [CellType] {
            var cells: [CellType] = [.backup]

            #if FEATURE_PRODUCTS
                cells.append(.apps)
            #endif

            #if FEATURE_SIGN_IN
                cells.append(.linkedDevices)
            #endif

            cells.append(.blockedUsers)

            return cells
        }
    }

    struct Content {
        let sections: [SettingsViewLayout.Section]
        let appVersion: String?

        static let empty = Content(sections: [], appVersion: nil)
    }
}
