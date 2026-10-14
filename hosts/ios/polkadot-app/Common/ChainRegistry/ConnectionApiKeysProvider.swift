import Foundation
import ChainRegistry

final class ConnectionApiKeysProvider: ConnectionApiKeysProviding {
    static let shared = ConnectionApiKeysProvider()

    private init() {}

    func getKey(by _: String) -> String? {
        nil
    }
}
