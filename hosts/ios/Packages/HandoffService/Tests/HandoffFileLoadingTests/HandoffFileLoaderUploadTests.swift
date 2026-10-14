import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
import AsyncExtensions
@testable import HandoffService

struct HandoffFileLoaderUploadTests {
    let chunkSize = 100
    let inlineMargin = 64
    let dummyPubKey = MultiSigner.sr25519(Data(repeating: 1, count: 32))

    var inlineThreshold: Int { chunkSize - inlineMargin }

    // MARK: - Chunked

    @Test func freshUploadCompletesSuccessfully() async throws {
        let fileData = try Data.randomOrError(of: 250)
        let (loader, service) = makeLoader()
        let store = MockUploadFileContext(fileData: fileData)

        let events = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        // 3 chunks (100 + 100 + 50) → 3 progress + 1 finished
        let progressEvents = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }
        let finishedEvents = events.compactMap { if case let .onFinished(f) = $0 { f } else { nil } }

        #expect(progressEvents.count == 3)
        #expect(finishedEvents.count == 1)

        // 3 chunks + 1 root entry = 4 submits
        #expect(service.submitCallCount == 4)

        // Each chunk saved to store
        #expect(store.savedChunks.count == 3)
        #expect(store.finishCalled == true)
    }

    @Test func uploadEmitsCorrectProgress() async throws {
        let fileData = try Data.randomOrError(of: 250)
        let (loader, _) = makeLoader()
        let store = MockUploadFileContext(fileData: fileData)

        let events = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        let progressValues = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }

        #expect(progressValues[0].uploaded == 100)
        #expect(progressValues[0].total == 250)
        #expect(progressValues[0].uploadedHashes.count == 1)

        #expect(progressValues[1].uploaded == 200)
        #expect(progressValues[1].uploadedHashes.count == 2)

        #expect(progressValues[2].uploaded == 250)
        #expect(progressValues[2].uploadedHashes.count == 3)
    }

    @Test func uploadResumesFromProgress() async throws {
        let fileData = try Data.randomOrError(of: 250)
        let (loader, service) = makeLoader()
        let store = MockUploadFileContext(fileData: fileData)

        // Simulate 2 chunks already uploaded
        let chunk0Hash = try fileData.subdata(in: 0 ..< 100).blake2b32()
        let chunk1Hash = try fileData.subdata(in: 100 ..< 200).blake2b32()

        store.resumeProgress = .init(
            uploadedHashes: [chunk0Hash, chunk1Hash],
            uploadedSize: 200
        )

        let events = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        let progressValues = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }

        // 1 initial resume progress + 1 chunk remaining = 2 progress events
        #expect(progressValues.count == 2)

        // Only 1 chunk submitted + 1 root entry = 2 submits
        #expect(service.submitCallCount == 2)

        // Only 1 new chunk saved
        #expect(store.savedChunks.count == 1)
        #expect(store.finishCalled == true)
    }

    @Test func uploadSavesChunkHashAfterEachSubmit() async throws {
        let fileData = try Data.randomOrError(of: 200)
        let (loader, _) = makeLoader()
        let store = MockUploadFileContext(fileData: fileData)

        _ = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        #expect(store.savedChunks.count == 2)
        #expect(store.savedChunks[0].uploadedSize == 100)
        #expect(store.savedChunks[1].uploadedSize == 200)
    }

    @Test func uploadCallsFinishFalseOnError() async throws {
        let fileData = try Data.randomOrError(of: 100)
        let service = MockHandoffService()
        service.submitError = TestError.intentional
        let (loader, _) = makeLoader(service: service)
        let store = MockUploadFileContext(fileData: fileData)

        let events = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        let hasError = events.contains { if case .onError = $0 { true } else { false } }

        #expect(hasError)
        #expect(store.finishCalled == false)
    }

    @Test func uploadRootEntryContainsAllChunkHashes() async throws {
        let fileData = try Data.randomOrError(of: 200)
        let (loader, service) = makeLoader()
        let store = MockUploadFileContext(fileData: fileData)

        let events = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        let finished = events.compactMap { if case let .onFinished(f) = $0 { f } else { nil } }.first

        guard case let .chunked(metadataHash) = finished else {
            Issue.record("Expected chunked finish, got \(String(describing: finished))")
            return
        }

        // Decode the stored root entry
        let metadataData = service.storage[metadataHash]!
        let envelope = try VersionedUploadedFile.scaleDecode(from: metadataData)

        guard case let .v1(.chunked(chunkedFile)) = envelope else {
            Issue.record("Expected chunked payload")
            return
        }

        #expect(chunkedFile.chunks.count == 2)
        #expect(chunkedFile.totalSize == 200)
    }

    // MARK: - Inline

    @Test func smallFileUploadsInline() async throws {
        let fileData = try Data.randomOrError(of: inlineThreshold)
        let (loader, service) = makeLoader()
        let store = MockUploadFileContext(fileData: fileData)

        let events = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        let progressValues = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }
        let finished = events.compactMap { if case let .onFinished(f) = $0 { f } else { nil } }.first

        guard case let .inline(fileHash) = finished else {
            Issue.record("Expected inline finish, got \(String(describing: finished))")
            return
        }

        // Single pool entry carrying the file itself, no chunk bookkeeping
        #expect(service.submitCallCount == 1)
        #expect(store.savedChunks.isEmpty)
        #expect(store.finishCalled == true)

        #expect(progressValues.count == 1)
        #expect(progressValues[0].uploaded == fileData.count)
        #expect(progressValues[0].total == fileData.count)

        let envelope = try VersionedUploadedFile.scaleDecode(from: service.storage[fileHash]!)
        #expect(envelope == .v1(.inline(fileData)))
    }

    @Test func fileAboveThresholdUploadsChunked() async throws {
        let fileData = try Data.randomOrError(of: inlineThreshold + 1)
        let (loader, service) = makeLoader()
        let store = MockUploadFileContext(fileData: fileData)

        let events = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        let finished = events.compactMap { if case let .onFinished(f) = $0 { f } else { nil } }.first

        guard case .chunked = finished else {
            Issue.record("Expected chunked finish, got \(String(describing: finished))")
            return
        }

        // 1 chunk + 1 root entry = 2 submits
        #expect(service.submitCallCount == 2)
        #expect(store.savedChunks.count == 1)
    }

    @Test func resumedUploadStaysChunkedBelowThreshold() async throws {
        let fileData = try Data.randomOrError(of: inlineThreshold)
        let (loader, service) = makeLoader()
        let store = MockUploadFileContext(fileData: fileData)

        // Progress persisted by a previous chunked run forces the chunked flow
        let chunkHash = try fileData.blake2b32()
        store.resumeProgress = .init(uploadedHashes: [chunkHash], uploadedSize: fileData.count)

        let events = try await collectUploadEvents(from: loader.uploadFile(
            store: store,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        ))

        let finished = events.compactMap { if case let .onFinished(f) = $0 { f } else { nil } }.first

        guard case .chunked = finished else {
            Issue.record("Expected chunked finish, got \(String(describing: finished))")
            return
        }

        // Only the root entry is submitted
        #expect(service.submitCallCount == 1)
    }
}

private extension HandoffFileLoaderUploadTests {
    func makeLoader(service: MockHandoffService = MockHandoffService()) -> (HandoffFileLoader, MockHandoffService) {
        let loader = HandoffFileLoader(
            service: service,
            config: HandoffFileLoadConfig(chunkSize: chunkSize, inlineMargin: inlineMargin)
        )
        return (loader, service)
    }

    func makeRecipients() -> FileRecipients {
        FileRecipients(pubKeys: [dummyPubKey], encryptor: PassthroughEncryptor())
    }

    func collectUploadEvents(
        from stream: AnyAsyncSequence<FileUploadingEvent>
    ) async throws -> [FileUploadingEvent] {
        var events: [FileUploadingEvent] = []
        for try await event in stream {
            events.append(event)
        }
        return events
    }
}
