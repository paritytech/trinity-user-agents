import Foundation

enum EnviromentVariables {
    static var isDebugEnabled: Bool {
        #if F_DEV
            true
        #else
            false
        #endif
    }

    static func variable(named name: String) -> String? {
        let processInfo = ProcessInfo.processInfo
        guard let value = processInfo.environment[name] else {
            return nil
        }
        return value
    }

    // MARK: - Build Environment

    enum BuildEnvironment {
        case debug
        case testFlight
        case production

        static var current: BuildEnvironment {
            guard !isDebugEnabled else {
                return .debug
            }
            guard Bundle.main.appStoreReceiptURL?.lastPathComponent == "sandboxReceipt" else {
                return .production
            }
            return .testFlight
        }

        var shouldLogSensitiveData: Bool {
            switch self {
            case .debug,
                 .testFlight:
                true
            case .production:
                false
            }
        }

        var name: String {
            switch self {
            case .debug: "debug"
            case .testFlight: "testflight"
            case .production: "production"
            }
        }
    }
}
