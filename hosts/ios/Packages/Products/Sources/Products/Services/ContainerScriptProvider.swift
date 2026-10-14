import Foundation

public protocol ContainerScriptProviding {
    func loadContainerScript() throws -> String
}

public final class BundledContainerScriptProvider: ContainerScriptProviding {
    public init() {}

    public func loadContainerScript() throws -> String {
        guard let url = Bundle.module.url(forResource: "container", withExtension: "js") else {
            throw ContainerScriptError.scriptNotFound
        }

        return try String(contentsOf: url, encoding: .utf8)
    }
}

public enum ContainerScriptError: Error, LocalizedError {
    case scriptNotFound

    public var errorDescription: String? {
        switch self {
        case .scriptNotFound:
            "container.js not found in app bundle"
        }
    }
}
