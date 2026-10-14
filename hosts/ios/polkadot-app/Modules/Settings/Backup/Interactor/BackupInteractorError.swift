import Foundation

enum BackupInteractorError: Error {
    case failedCreateBackup

    var localizedDescription: String {
        switch self {
        case .failedCreateBackup: String(localized: .backupInfoFailed)
        }
    }
}
