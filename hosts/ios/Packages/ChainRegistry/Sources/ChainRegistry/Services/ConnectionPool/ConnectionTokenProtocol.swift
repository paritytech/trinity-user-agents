import Foundation

public protocol ConnectionTokenProtocol: Sendable {
    func release()
}
