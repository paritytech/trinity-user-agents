import Foundation

enum RemoteConfigError: Error {
    case versionNotFound
    case chainsNotFound
    case invalidConfig
    case unknownError
}
