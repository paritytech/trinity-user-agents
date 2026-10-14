import Foundation

protocol QRScannerErrorDisplayFactoryProtocol {
    func createStringQRExtraction(error: QRExtractionServiceError, locale: Locale) -> String
    func createStringCapture(error: QRCaptureServiceError, locale: Locale) -> String
    func createMatcherFailedString(for locale: Locale) -> String
}

extension QRScannerErrorDisplayFactoryProtocol {
    func createStringQRExtraction(error: QRExtractionServiceError) -> String {
        createStringQRExtraction(error: error, locale: .current)
    }

    func createStringCapture(error: QRCaptureServiceError) -> String {
        createStringCapture(error: error, locale: .current)
    }

    func createMatcherFailedString() -> String {
        createMatcherFailedString(for: .current)
    }
}
