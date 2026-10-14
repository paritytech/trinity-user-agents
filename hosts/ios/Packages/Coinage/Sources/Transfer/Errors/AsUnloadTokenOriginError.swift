import Foundation

enum AsUnloadTokenOriginError: Error {
    case emptyRingKeys
    case memberNotIncluded
    case resolverRequired
}
