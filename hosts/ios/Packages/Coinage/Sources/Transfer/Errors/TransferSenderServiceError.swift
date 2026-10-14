import Foundation

/// Errors from TransferSenderService.
enum TransferSenderServiceError: Error {
    case noSuitableCoins
    case notConfigured
    case planCreationFailed(Error)
    case strategyFailed(Error)
    case memoBuildingFailed(Error)
}
