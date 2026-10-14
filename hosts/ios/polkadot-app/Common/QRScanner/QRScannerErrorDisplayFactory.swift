import Foundation

final class QRScannerErrorDisplayFactory {}

extension QRScannerErrorDisplayFactory: QRScannerErrorDisplayFactoryProtocol {
    func createStringQRExtraction(error: QRExtractionServiceError, locale _: Locale) -> String {
        switch error {
        case .noFeatures:
            String(localized: .QRScan.errorNoInfo)
        case .detectorUnavailable,
             .invalidImage:
            String(localized: .QRScan.errorInvalidImage)
        }
    }

    func createStringCapture(error: QRCaptureServiceError, locale _: Locale) -> String {
        switch error {
        case .deviceAccessRestricted:
            String(localized: .QRScan.errorCameraRestricted)
        case .unsupportedFormat:
            String(localized: .QRScan.unsupported)
        default:
            ""
        }
    }

    func createMatcherFailedString(for _: Locale) -> String {
        String(localized: .QRScan.errorNoInfo)
    }
}
