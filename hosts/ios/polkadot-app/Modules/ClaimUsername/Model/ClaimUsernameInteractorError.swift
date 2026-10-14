import Foundation

enum ClaimUsernameInteractorError: Error {
    case claimFailed(Error)
    case claimTimeout
}
