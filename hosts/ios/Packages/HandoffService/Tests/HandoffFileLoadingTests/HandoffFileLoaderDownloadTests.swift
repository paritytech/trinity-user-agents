import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
import AsyncExtensions
@testable import HandoffService

struct HandoffFileLoaderDownloadTests {
    let chunkSize = 100

    // MARK: - Chunked

    @Test func freshDownloadCompletesSuccessfully() async throws {
        let fileData = try Data.randomOrError(of: 250)
        let (loader, service) = makeLoader()
        let entryHash = try populateService(service, with: fileData)
        let store = MockDownloadFileContext(entryHash: entryHash)

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        let progressEvents = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }
        let finishedEvents = events.compactMap { if case let .onFinished(h) = $0 { h } else { nil } }

        #expect(progressEvents.count == 3)
        #expect(finishedEvents.count == 1)
        #expect(finishedEvents.first == entryHash)

        // All chunks downloaded and assembled correctly
        #expect(store.assembleFile() == fileData)
        #expect(store.finishCalled == true)
    }

    @Test func downloadSavesEntryOnFreshStart() async throws {
        let fileData = try Data.randomOrError(of: 100)
        let (loader, service) = makeLoader()
        let entryHash = try populateService(service, with: fileData)
        let store = MockDownloadFileContext(entryHash: entryHash)

        _ = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        #expect(store.savedMetadata != nil)
        #expect(store.savedTotalChunks == 1)
    }

    @Test func downloadAcknowledgesAfterEachChunk() async throws {
        let fileData = try Data.randomOrError(of: 200)
        let (loader, service) = makeLoader()
        let entryHash = try populateService(service, with: fileData)
        let store = MockDownloadFileContext(entryHash: entryHash)

        _ = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        // 1 root entry ack + 2 chunk acks
        #expect(service.ackedHashes.count == 3)
        #expect(service.ackedHashes.first == entryHash)
    }

    @Test func downloadEmitsCorrectProgress() async throws {
        let fileData = try Data.randomOrError(of: 250)
        let (loader, service) = makeLoader()
        let entryHash = try populateService(service, with: fileData)
        let store = MockDownloadFileContext(entryHash: entryHash)

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        let progress = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }

        #expect(progress[0].downloaded == 100)
        #expect(progress[0].total == 250)
        #expect(progress[1].downloaded == 200)
        #expect(progress[2].downloaded == 250)
    }

    @Test func downloadResumesFromLastChunk() async throws {
        let fileData = try Data.randomOrError(of: 300)
        let (loader, service) = makeLoader()
        let entryHash = try populateService(service, with: fileData)
        let store = MockDownloadFileContext(entryHash: entryHash)

        // Build resume metadata matching what was stored
        let metadata = service.storage[entryHash]!

        store.resumeInfo = .chunked(.init(
            metadata: metadata,
            lastChunkIndex: 1,
            downloadedBytes: 200
        ))

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        let progress = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }

        // 1 initial resume progress + 1 remaining chunk = 2 progress events
        #expect(progress.count == 2)
        #expect(progress[0].downloaded == 200)
        #expect(progress[1].downloaded == 300)

        // Only 1 chunk claimed (chunk index 2), no root entry claim
        #expect(service.claimCallCount == 1)
        #expect(store.appendedChunks.count == 1)
        #expect(store.appendedChunks[0].index == 2)
        #expect(store.savedEntry == nil)
    }

    @Test func downloadReturnsErrorForMissingEntry() async throws {
        let (loader, _) = makeLoader()
        let fakeHash = try Data.randomOrError(of: 32)
        let store = MockDownloadFileContext(entryHash: fakeHash)

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: fakeHash,
            claimer: makeClaimer(),
            store: store
        ))

        let hasNoEntryError = events.contains {
            if case let .onError(error) = $0,
               let downloadError = error as? FileDownloadingError,
               case .noEntry = downloadError {
                true
            } else {
                false
            }
        }

        #expect(hasNoEntryError)
    }

    @Test func downloadReturnsErrorForMissingChunk() async throws {
        let fileData = try Data.randomOrError(of: 200)
        let (loader, service) = makeLoader()
        let entryHash = try populateService(service, with: fileData)

        // Remove one chunk from the service so claim returns nil
        let metadata = service.storage[entryHash]!

        guard case let .v1(.chunked(chunkedFile)) = try VersionedUploadedFile.scaleDecode(from: metadata) else {
            Issue.record("Unexpected entry payload")
            return
        }

        service.storage.removeValue(forKey: chunkedFile.chunks[1])

        let store = MockDownloadFileContext(entryHash: entryHash)

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        let hasNoChunkError = events.contains {
            if case let .onError(error) = $0,
               let downloadError = error as? FileDownloadingError,
               case .noChunk = downloadError {
                true
            } else {
                false
            }
        }

        #expect(hasNoChunkError)
        #expect(store.finishCalled == false)
    }

    @Test func downloadCallsFinishFalseOnError() async throws {
        let service = MockHandoffService()
        service.claimError = TestError.intentional
        let (loader, _) = makeLoader(service: service)
        let fakeHash = try Data.randomOrError(of: 32)
        let store = MockDownloadFileContext(entryHash: fakeHash)

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: fakeHash,
            claimer: makeClaimer(),
            store: store
        ))

        let hasError = events.contains { if case .onError = $0 { true } else { false } }

        #expect(hasError)
        #expect(store.finishCalled == false)
    }

    @Test func downloadAppendsChunksInOrder() async throws {
        let fileData = try Data.randomOrError(of: 250)
        let (loader, service) = makeLoader()
        let entryHash = try populateService(service, with: fileData)
        let store = MockDownloadFileContext(entryHash: entryHash)

        _ = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        #expect(store.appendedChunks.count == 3)
        #expect(store.appendedChunks[0].index == 0)
        #expect(store.appendedChunks[1].index == 1)
        #expect(store.appendedChunks[2].index == 2)
    }

    // MARK: - Inline

    @Test func inlineDownloadCompletesInSingleRoundTrip() async throws {
        let fileData = try Data.randomOrError(of: 36)
        let (loader, service) = makeLoader()
        let entryHash = try populateService(service, withInline: fileData)
        let store = MockDownloadFileContext(entryHash: entryHash)

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        let progress = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }
        let finishedEvents = events.compactMap { if case let .onFinished(h) = $0 { h } else { nil } }

        #expect(progress.count == 1)
        #expect(progress[0].downloaded == fileData.count)
        #expect(progress[0].total == fileData.count)
        #expect(finishedEvents == [entryHash])

        // File persisted via saveEntry with no chunk append round-trips
        #expect(store.savedInlineData == fileData)
        #expect(store.appendedChunks.isEmpty)
        #expect(store.finishCalled == true)

        // One claim + one ack, and the ack follows persistence
        #expect(service.claimCallCount == 1)
        #expect(service.ackedHashes == [entryHash])
    }

    @Test func inlineDownloadResumesWithoutNetworkCalls() async throws {
        let (loader, service) = makeLoader()
        let entryHash = try Data.randomOrError(of: 32)
        let store = MockDownloadFileContext(entryHash: entryHash)

        store.resumeInfo = .inline(downloadedBytes: 42)

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        let progress = events.compactMap { if case let .onProgress(p) = $0 { p } else { nil } }
        let finishedEvents = events.compactMap { if case let .onFinished(h) = $0 { h } else { nil } }

        #expect(progress.count == 1)
        #expect(progress[0].downloaded == 42)
        #expect(finishedEvents == [entryHash])

        #expect(service.claimCallCount == 0)
        #expect(service.ackedHashes.isEmpty)
        #expect(store.savedEntry == nil)
        #expect(store.finishCalled == true)
    }

    // MARK: - Legacy

    @Test func downloadFailsForLegacyUnversionedEntry() async throws {
        let fileData = try Data.randomOrError(of: 200)
        let (loader, service) = makeLoader()

        // Pre-RFC root entry: bare ChunkedFile layout with no envelope
        let chunkHash = try fileData.blake2b32()
        let legacyEntry = try ChunkedFile(totalSize: UInt64(fileData.count), chunks: [chunkHash]).scaleEncoded()
        let entryHash = try legacyEntry.blake2b32()
        service.storage[entryHash] = legacyEntry

        let store = MockDownloadFileContext(entryHash: entryHash)

        let events = try await collectDownloadEvents(from: loader.downloadFile(
            using: entryHash,
            claimer: makeClaimer(),
            store: store
        ))

        let hasDecodingError = events.contains {
            if case let .onError(error) = $0, error is UploadedFileDecodingError {
                true
            } else {
                false
            }
        }

        #expect(hasDecodingError)
        #expect(store.savedEntry == nil)
        #expect(store.finishCalled == false)
    }
}

private extension HandoffFileLoaderDownloadTests {
    func makeLoader(
        service: MockHandoffService = MockHandoffService()
    ) -> (HandoffFileLoader, MockHandoffService) {
        let loader = HandoffFileLoader(
            service: service,
            config: HandoffFileLoadConfig(chunkSize: chunkSize)
        )
        return (loader, service)
    }

    func makeClaimer() -> FileClaimer {
        FileClaimer(
            proofProvider: MockRecipientProofProvider(),
            decryptor: PassthroughEncryptor()
        )
    }

    func populateService(
        _ service: MockHandoffService,
        with fileData: Data
    ) throws -> Data {
        let chunks = fileData.chunked(by: chunkSize)

        var chunkHashes: [Data] = []

        for chunk in chunks {
            let hash = try chunk.blake2b32()
            service.storage[hash] = chunk
            chunkHashes.append(hash)
        }

        let chunkedFile = ChunkedFile(totalSize: UInt64(fileData.count), chunks: chunkHashes)
        let entry = try VersionedUploadedFile.v1(.chunked(chunkedFile)).scaleEncoded()
        let entryHash = try entry.blake2b32()
        service.storage[entryHash] = entry

        return entryHash
    }

    func populateService(
        _ service: MockHandoffService,
        withInline fileData: Data
    ) throws -> Data {
        let entry = try VersionedUploadedFile.v1(.inline(fileData)).scaleEncoded()
        let entryHash = try entry.blake2b32()
        service.storage[entryHash] = entry

        return entryHash
    }

    func collectDownloadEvents(
        from stream: AnyAsyncSequence<FileDownloadingEvent>
    ) async throws -> [FileDownloadingEvent] {
        var events: [FileDownloadingEvent] = []
        for try await event in stream {
            events.append(event)
        }
        return events
    }
}
