import Foundation
import FoundationExt

public enum IpfsFetcherError: Error {
    case invalidHash
}

public protocol IpfsFetching {
    func lookupBy(rawHash: Data) async throws -> Data

    /// For content already addressed by CID, such as a product manifest's icon.
    func lookupBy(cid: String) async throws -> Data
}

public final class IpfsFetcher: IpfsFetching {
    private let converter: HexToCIDConverting
    private let session: URLSession

    public init(ipfsBaseURL: URL, session: URLSession = URLSession.shared) {
        converter = HexToCIDConverter(ipfsBaseURL: ipfsBaseURL)
        self.session = session
    }

    public func lookupBy(rawHash: Data) async throws -> Data {
        guard let url = converter.convertToIPFSURL(hash: rawHash, codec: .raw) else {
            throw IpfsFetcherError.invalidHash
        }

        return try await fetch(url)
    }

    public func lookupBy(cid: String) async throws -> Data {
        try await fetch(converter.ipfsURL(cid: cid))
    }
}

private extension IpfsFetcher {
    func fetch(_ url: URL) async throws -> Data {
        let (data, response) = try await session.data(from: url)
        try response.ensureSuccess()

        return data
    }
}
