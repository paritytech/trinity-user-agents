import Foundation
import FoundationExt

public protocol ProductScriptDownloaderProtocol {
    func download(url: String) async throws -> String
}

public final class HTTPProductScriptDownloader: ProductScriptDownloaderProtocol {
    private let session: URLSession

    public init(session: URLSession = .shared) {
        self.session = session
    }

    public func download(url: String) async throws -> String {
        guard let requestURL = URL(string: url) else {
            throw ScriptDownloadError.invalidURL(url)
        }

        let (data, response) = try await session.data(from: requestURL)

        try response.ensureSuccess()

        guard let content = String(data: data, encoding: .utf8) else {
            throw ScriptDownloadError.invalidEncoding
        }

        return content
    }
}

public enum ScriptDownloadError: Error, LocalizedError {
    case invalidURL(String)
    case invalidEncoding

    public var errorDescription: String? {
        switch self {
        case let .invalidURL(url):
            "Invalid script URL: \(url)"
        case .invalidEncoding:
            "Script content is not valid UTF-8"
        }
    }
}
