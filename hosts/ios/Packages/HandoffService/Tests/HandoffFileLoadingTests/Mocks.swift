import Foundation
import HandoffService
import SubstrateSdk
import NovaCrypto

// MARK: - Service

final class MockHandoffService: HandoffServicing, @unchecked Sendable {
    var storage: [Data: Data] = [:]
    private(set) var ackedHashes: [Data] = []
    private(set) var submitCallCount = 0
    private(set) var claimCallCount = 0

    var submitError: Error?
    var claimError: Error?
    var claimErrorsByHash: [Data: Error] = [:]

    @discardableResult
    func submitData(
        _ data: Data,
        from _: SenderProofProviding,
        recipients _: Set<MultiSigner>
    ) async throws -> SubmittedData {
        if let error = submitError { throw error }

        let hash = try data.blake2b32()
        storage[hash] = data
        submitCallCount += 1

        return SubmittedData(
            hash: hash,
            poolStatus: PoolStatus(entryCount: 1, totalBytes: data.count, maxBytes: 10_000_000)
        )
    }

    func claimData(
        by dataHash: Data,
        recipient _: RecipientProofProviding
    ) async throws -> Data? {
        if let error = claimError { throw error }
        if let error = claimErrorsByHash[dataHash] { throw error }
        claimCallCount += 1
        return storage[dataHash]
    }

    func acknowledgeReceivedData(
        by dataHash: Data,
        recipient _: RecipientProofProviding
    ) async throws {
        ackedHashes.append(dataHash)
    }
}

// MARK: - JSON-RPC Engine

final class MockJSONRPCEngine: JSONRPCEngine, @unchecked Sendable {
    let callbackQueue: DispatchQueue

    var result: Result<String, Error>?
    private(set) var lastMethod: String?
    private(set) var lastParams: [String]?

    init(callbackQueue: DispatchQueue = DispatchQueue(label: "io.parity.tests.mock-jsonrpc-engine")) {
        self.callbackQueue = callbackQueue
    }

    func callMethod<T: Decodable>(
        _ method: String,
        params: (some Encodable)?,
        options _: JSONRPCOptions,
        completion closure: ((Result<T, Error>) -> Void)?
    ) throws -> UInt16 {
        lastMethod = method

        if let params, let encoded = try? JSONEncoder().encode(params) {
            lastParams = try? JSONDecoder().decode([String].self, from: encoded)
        }

        let callResult: Result<T, Error> =
            switch result {
            case let .success(value):
                if let typed = value as? T {
                    .success(typed)
                } else {
                    .failure(TestError.intentional)
                }
            case let .failure(error):
                .failure(error)
            case nil:
                .failure(TestError.intentional)
            }

        callbackQueue.async {
            closure?(callResult)
        }

        return 0
    }

    func subscribe<T: Decodable>(
        _: String,
        params _: (some Encodable)?,
        unsubscribeMethod _: String,
        options _: JSONRPCOptions,
        onSubscribed _: ((JSONRPCSubscriptionId) -> Void)?,
        updateClosure _: @escaping (T) -> Void,
        failureClosure _: @escaping (Error, Bool) -> Void
    ) throws -> UInt16 {
        throw TestError.intentional
    }

    func cancelForIdentifiers(_: [UInt16], sendUnsubscribe _: Bool) {}

    func addBatchCallMethod(_: String, params _: (some Encodable)?, batchId _: JSONRPCBatchId) throws {
        throw TestError.intentional
    }

    func submitBatch(
        for _: JSONRPCBatchId,
        options _: JSONRPCOptions,
        completion _: (([Result<JSON, Error>]) -> Void)?
    ) throws -> [UInt16] {
        throw TestError.intentional
    }

    func clearBatch(for _: JSONRPCBatchId) {}
}

// MARK: - Remote Store

final class MockLongTermRemoteStore: LongTermRemoteStoring, @unchecked Sendable {
    var storage: [Data: Data] = [:]
    var error: Error?
    private(set) var requestedHashes: [Data] = []

    func downloadData(by fileHash: FileHash) async throws -> Data? {
        if let error { throw error }
        requestedHashes.append(fileHash)
        return storage[fileHash]
    }
}

// MARK: - Encryption

final class PassthroughEncryptor: FileEncrypting {
    func encrypt(_ data: Data) throws -> Data { data }
    func decrypt(_ encryptedData: Data) throws -> Data { encryptedData }
}

// MARK: - Proof

final class NoProofProvider: SenderProofProviding {
    func getProof(for _: FileHash) async throws -> SenderProof {
        SenderProof(
            sender: .sr25519(Data(repeating: 0, count: 32)),
            signature: .sr25519(data: Data(repeating: 0, count: 64)),
            submitTimestamp: 0
        )
    }
}

final class MockRecipientProofProvider: RecipientProofProviding {
    func getProof(for _: Data, context _: Data) async throws -> MultiSignature {
        .sr25519(data: Data(repeating: 0, count: 64))
    }
}

// MARK: - Upload Store

final class MockUploadFileContext: UploadFileContextProtocol, @unchecked Sendable {
    let fileData: Data

    private(set) var savedChunks: [(hash: Data, uploadedSize: Int64)] = []
    private(set) var finishCalled: Bool?

    var resumeProgress: ResumeUploadInfo.Progress?

    init(fileData: Data) {
        self.fileData = fileData
    }

    func fetchResumeInfo() async throws -> ResumeUploadInfo {
        ResumeUploadInfo(fileSize: fileData.count, progress: resumeProgress)
    }

    func fetchChunk(after bytesCount: Int, size: Int) async throws -> Data {
        let end = min(bytesCount + size, fileData.count)
        return fileData.subdata(in: bytesCount ..< end)
    }

    func saveUploadedChunk(_ hash: Data, uploadedSize: Int64) async throws {
        savedChunks.append((hash: hash, uploadedSize: uploadedSize))
    }

    func finishUploading(_ fullFileUploaded: Bool) async throws {
        finishCalled = fullFileUploaded
    }
}

// MARK: - Download Store

final class MockDownloadFileContext: DownloadFileContextProtocol, @unchecked Sendable {
    let entryHash: FileHash

    private(set) var savedEntry: DownloadedEntry?
    private(set) var appendedChunks: [(data: Data, index: Int)] = []
    private(set) var finishCalled: Bool?

    var resumeInfo: ResumeDownloadInfo?

    init(entryHash: FileHash) {
        self.entryHash = entryHash
    }

    var savedMetadata: Data? {
        if case let .chunked(metadata, _) = savedEntry { metadata } else { nil }
    }

    var savedTotalChunks: Int? {
        if case let .chunked(_, totalChunks) = savedEntry { totalChunks } else { nil }
    }

    var savedInlineData: Data? {
        if case let .inline(fileData) = savedEntry { fileData } else { nil }
    }

    func saveEntry(_ entry: DownloadedEntry) async throws {
        savedEntry = entry
    }

    func fetchResumeInfo() async throws -> ResumeDownloadInfo? {
        resumeInfo
    }

    func appendChunk(_ data: Data, at index: Int) async throws {
        appendedChunks.append((data: data, index: index))
    }

    func finishDownloading(_ fullFileDownloaded: Bool) async throws {
        finishCalled = fullFileDownloaded
    }

    func assembleFile() -> Data {
        if let savedInlineData {
            return savedInlineData
        }

        return appendedChunks.sorted { $0.index < $1.index }
            .reduce(Data()) { $0 + $1.data }
    }
}

// MARK: - Helpers

enum TestError: Error {
    case intentional
}
