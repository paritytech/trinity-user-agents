import Foundation

public protocol ConnectionApiKeysProviding {
    func getKey(by name: String) -> String?
}
