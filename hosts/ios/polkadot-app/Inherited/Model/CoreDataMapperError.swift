import Foundation

enum CoreDataMapperError: Error {
    case missingRequiredData(keyPath: String)
    case unexpected(String)
    case unsupported
}
