import Foundation
import Foundation_iOS
import NovaCrypto
import SubstrateSdk

public typealias FileTicket = Data
public typealias FileHash = Data

private extension FileTicket {
    static let ticketSize = 32
    static let ticketSignerContext = Data("signer".utf8)
    static let ticketEncryptorContext = Data("encryption".utf8)
}

public extension FileTicket {
    static func generateFileTicket() throws -> FileTicket {
        try FileTicket.randomOrError(of: ticketSize)
    }

    func deriveSigningKeypair() throws -> SNKeypairProtocol {
        let seed = try Self.ticketSignerContext.blake2b32WithKey(self)
        return try SNKeyFactory().createKeypair(fromSeed: seed)
    }

    func deriveMultiSigner() throws -> MultiSigner {
        let pubKey = try deriveSigningKeypair().publicKey().rawData()
        return .sr25519(pubKey)
    }

    func deriveEncryptor() throws -> FileEncrypting {
        let rawEncryptionKey = try Self.ticketEncryptorContext.blake2b32WithKey(self)
        return ChaChaPolyFileEncryptor(rawKey: rawEncryptionKey)
    }
}

public enum FileUploadingEvent {
    public struct Progress: Equatable {
        public let uploaded: Int
        public let total: Int
        public let uploadedHashes: [Data]

        public init(uploaded: Int, total: Int, uploadedHashes: [Data]) {
            self.uploaded = uploaded
            self.total = total
            self.uploadedHashes = uploadedHashes
        }
    }

    public enum Finished: Equatable {
        case inline(file: FileHash)
        case chunked(metadata: FileHash)

        public var entryHash: FileHash {
            switch self {
            case let .inline(file):
                file
            case let .chunked(metadata):
                metadata
            }
        }
    }

    case onProgress(Progress)
    case onFinished(Finished)
    case onError(Error)
}

// Versioned pool root entry envelope defined by chat RFC 0001. Wire indices are normative.
enum VersionedUploadedFile: ScaleCodable, Equatable {
    case v1(UploadedFile)

    init(scaleDecoder: ScaleDecoding) throws {
        let version = try UInt8(scaleDecoder: scaleDecoder)

        switch version {
        case 0:
            self = try .v1(UploadedFile(scaleDecoder: scaleDecoder))
        default:
            throw UploadedFileDecodingError.unsupportedVersion(version)
        }
    }

    func encode(scaleEncoder: ScaleEncoding) throws {
        switch self {
        case let .v1(payload):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try payload.encode(scaleEncoder: scaleEncoder)
        }
    }
}

enum UploadedFile: ScaleCodable, Equatable {
    case inline(Data)
    case chunked(ChunkedFile)

    init(scaleDecoder: ScaleDecoding) throws {
        let payloadIndex = try UInt8(scaleDecoder: scaleDecoder)

        switch payloadIndex {
        case 0:
            self = try .inline(Data(scaleDecoder: scaleDecoder))
        case 1:
            self = try .chunked(ChunkedFile(scaleDecoder: scaleDecoder))
        default:
            throw UploadedFileDecodingError.unsupportedPayload(payloadIndex)
        }
    }

    func encode(scaleEncoder: ScaleEncoding) throws {
        switch self {
        case let .inline(fileData):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try fileData.encode(scaleEncoder: scaleEncoder)
        case let .chunked(chunkedFile):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try chunkedFile.encode(scaleEncoder: scaleEncoder)
        }
    }
}

struct ChunkedFile: ScaleCodable, Equatable {
    let totalSize: UInt64
    let chunks: [Data]

    init(totalSize: UInt64, chunks: [Data]) {
        self.totalSize = totalSize
        self.chunks = chunks
    }

    init(scaleDecoder: ScaleDecoding) throws {
        totalSize = try UInt64(scaleDecoder: scaleDecoder)
        chunks = try [Data](scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: ScaleEncoding) throws {
        try totalSize.encode(scaleEncoder: scaleEncoder)
        try chunks.encode(scaleEncoder: scaleEncoder)
    }
}

public enum UploadedFileDecodingError: Error, Equatable {
    case unsupportedVersion(UInt8)
    case unsupportedPayload(UInt8)
}

public enum FileDownloadingEvent {
    public struct Progress: Equatable {
        public let downloaded: Int
        public let total: Int

        public init(downloaded: Int, total: Int) {
            self.downloaded = downloaded
            self.total = total
        }
    }

    case onProgress(Progress)
    case onFinished(FileHash)
    case onError(Error)
}

public enum FileDownloadingError: Error {
    case noEntry(FileHash)
    case noChunk(FileHash)
    case invalidResumeMetadata
    case unexpectedChunkedPayload(FileHash)
}

public enum FileUploadingError: Error {
    case inlineLimitExceeded(size: Int, limit: Int)
}
