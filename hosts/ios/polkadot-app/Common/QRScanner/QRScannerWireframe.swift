import Foundation

@MainActor
final class QRScannerWireframe: QRScannerWireframeProtocol, ApplicationSettingsPresentable {
    func askOpenSettings(from view: QRScannerViewProtocol?, completion: ((ApplicationSettingsAlertResult) -> Void)?) {
        askOpenApplicationSettings(
            with: String(localized: .QRScan.errorCameraRestrictedPreviously),
            title: String(localized: .QRScan.errorCameraTitle),
            from: view,
            completion: completion
        )
    }
}
