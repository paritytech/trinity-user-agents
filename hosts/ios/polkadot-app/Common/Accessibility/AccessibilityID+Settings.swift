import ExternalAccessibility

extension AccessibilityID.Settings {
    static func menuItem(for cellType: SettingsViewModel.CellType) -> (any AccessibilityIdentifying)? {
        switch cellType {
        case .backup:
            backupButton
        default:
            nil
        }
    }
}
