import Foundation

public struct WKWebViewJSEngineSettings {
    public let usesPersistentLocalStorage: Bool
    public let applicationNameForUserAgent: String?

    public init(usesPersistentLocalStorage: Bool = true, applicationNameForUserAgent: String? = nil) {
        self.usesPersistentLocalStorage = usesPersistentLocalStorage
        self.applicationNameForUserAgent = applicationNameForUserAgent
    }
}
