import Foundation

public struct WKWebViewJSEngineSettings {
    public let usesPersistentLocalStorage: Bool

    public init(usesPersistentLocalStorage: Bool = true) {
        self.usesPersistentLocalStorage = usesPersistentLocalStorage
    }
}
