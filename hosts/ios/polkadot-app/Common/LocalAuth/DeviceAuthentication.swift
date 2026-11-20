import Foundation
import LocalAuthentication

protocol DeviceAuthProtocol {
    var isAvailable: Bool { get }

    func authenticate(
        localizedReason: String,
        completionQueue: DispatchQueue,
        completionBlock: @escaping (Result<Bool, DeviceAuthError>) -> Void
    )
}

enum DeviceAuthError: Error {
    case authFailed
    case notAvailable
    case cancelled
    case unknown(String)
}

class DeviceAuthentication {
    private lazy var context = LAContext()

    private func converError(error: Error) -> DeviceAuthError? {
        guard let laError = error as? LAError else {
            return .unknown(error.localizedDescription)
        }

        switch laError {
        case LAError.authenticationFailed:
            return .authFailed
        case LAError.biometryNotAvailable,
             LAError.biometryNotEnrolled,
             LAError.passcodeNotSet:
            return .notAvailable
        case LAError.appCancel,
             LAError.systemCancel:
            return .cancelled
        case LAError.userCancel:
            return nil
        default:
            return .unknown(error.localizedDescription)
        }
    }
}

extension DeviceAuthentication: DeviceAuthProtocol {
    var isAvailable: Bool {
        context.canEvaluatePolicy(LAPolicy.deviceOwnerAuthentication, error: nil)
    }

    func authenticate(
        localizedReason: String,
        completionQueue: DispatchQueue,
        completionBlock: @escaping (Result<Bool, DeviceAuthError>) -> Void
    ) {
        guard isAvailable else {
            completionQueue.async {
                completionBlock(.failure(.notAvailable))
            }
            return
        }

        context.evaluatePolicy(
            LAPolicy.deviceOwnerAuthentication,
            localizedReason: localizedReason
        ) { [weak self] (result: Bool, error: Error?) in
            completionQueue.async {
                if result {
                    completionBlock(.success(true))
                } else if let error, let convertedError = self?.converError(error: error) {
                    completionBlock(.failure(convertedError))
                } else {
                    completionBlock(.success(false))
                }
            }
        }
    }
}
