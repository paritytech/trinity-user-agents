import Foundation

struct BackupViewModel {
    enum BackupStatusType {
        case created
        case notFound
        case cloudIsOff
    }

    let statusType: BackupStatusType
}
