import Foundation
import BulletinChain

protocol PolkadotHandshakeMetadataMaking {
    func makeMetadata(from hostData: HandshakeProposal) async throws -> HandshakeMetadata
}

final class PolkadotHandshakeMetadataFactory {}

extension PolkadotHandshakeMetadataFactory: PolkadotHandshakeMetadataMaking {
    func makeMetadata(from hostData: HandshakeProposal) async throws -> HandshakeMetadata {
        switch hostData {
        case let .v1(data):
            try await makeMetadataV1(urlString: data.metadata)
        case let .v2(data):
            try makeMetadataV2(from: data)
        }
    }
}

private extension PolkadotHandshakeMetadataFactory {
    struct MetadataRequestResult: Decodable {
        let name: String
        let icon: URL?
    }

    enum MetadataError: Error {
        case invalidMetadataURL
        case missingHostName
    }

    func makeMetadataV1(urlString: String) async throws -> HandshakeMetadata {
        guard let url = URL(string: urlString) else {
            throw MetadataError.invalidMetadataURL
        }

        let data = try await performRequest(with: url)
        let decoder = JSONDecoder()
        let requestResult = try decoder.decode(MetadataRequestResult.self, from: data)

        return HandshakeMetadata(name: requestResult.name, iconUrl: requestResult.icon)
    }

    func makeMetadataV2(
        from data: HandshakeProposal.DataV2
    ) throws -> HandshakeMetadata {
        guard let name = data.hostName else {
            throw MetadataError.missingHostName
        }

        let iconUrl = makeIconUrl(forCID: data.hostIconCID)
        return HandshakeMetadata(name: name, iconUrl: iconUrl)
    }

    func makeIconUrl(forCID cid: String?) -> URL? {
        guard let cidString = cid else {
            return nil
        }

        let converter = HexToCIDConverter(ipfsBaseURL: AppConfig.KnownIPFS.main)
        return converter.convertToIPFSURL(fileHash: cidString, codec: .raw)
    }

    func performRequest(with url: URL) async throws -> Data {
        let (data, response) = try await URLSession.shared.data(from: url)
        try response.ensureSuccess()
        return data
    }
}
