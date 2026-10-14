import Foundation
import Testing
import SDKLogger
import StructuredConcurrency

@testable import polkadot_app

@Suite("MixnetDownloadContext")
struct MixnetDownloadContextTests {
    private func makeContext(maxConcurrent: Int = 2) -> MixnetDownloadContext {
        MixnetDownloadContext(
            maxConcurrentDownloads: maxConcurrent,
            logger: Logger.shared
        )
    }

    private func makeRunningTask() -> Task<Void, Never> {
        Task {}
    }

    private func makeDownloadData(id: String) -> MixnetDownloadData {
        MixnetDownloadData(
            fileVariant: .p2pMixnet(.init(
                identifier: Data(id.utf8),
                claimTicket: Data(),
                node: .wssUrl("wss://test"),
                meta: .general(.init(mimeType: "jpg", fileSize: 100))
            )),
            attachmentId: AttachmentId(messageId: "msg", fileId: id)
        )
    }

    // MARK: - processDownloadData

    @Test("starts task immediately when under capacity")
    func startsImmediately() async {
        let context = makeContext(maxConcurrent: 2)
        let started = AnyObjectHolder<Bool>()
        started.set(false)

        await context.processDownloadData(for: makeDownloadData(id: "f1")) {
            started.set(true)
            return makeRunningTask()
        }

        #expect(started.get()!)
        let taskCount = await context.tasks.count
        #expect(taskCount == 1)
    }

    @Test("enqueues when at capacity")
    func enqueuesAtCapacity() async {
        let context = makeContext(maxConcurrent: 1)
        let secondStarted = AnyObjectHolder<Bool>()
        secondStarted.set(false)

        await context.processDownloadData(for: makeDownloadData(id: "f1")) { makeRunningTask() }

        await context.processDownloadData(for: makeDownloadData(id: "f2")) {
            secondStarted.set(true)
            return makeRunningTask()
        }

        #expect(secondStarted.get() == false)
        let pendingCount = await context.pendingTasks.count
        #expect(pendingCount == 1)
    }

    @Test("ignores duplicate in active tasks")
    func ignoresDuplicateActive() async {
        let context = makeContext(maxConcurrent: 2)
        let callCount = AnyObjectHolder<Int>()
        callCount.set(0)

        let data = makeDownloadData(id: "f1")

        await context.processDownloadData(for: data) {
            callCount.set((callCount.get() ?? 0) + 1)
            return makeRunningTask()
        }

        await context.processDownloadData(for: data) {
            callCount.set((callCount.get() ?? 0) + 1)
            return makeRunningTask()
        }

        #expect(callCount.get() == 1)
    }

    @Test("ignores duplicate in pending queue")
    func ignoresDuplicatePending() async {
        let context = makeContext(maxConcurrent: 1)

        await context.processDownloadData(for: makeDownloadData(id: "f1")) { makeRunningTask() }

        let duplicateData = makeDownloadData(id: "f2")
        await context.processDownloadData(for: duplicateData) { makeRunningTask() }
        await context.processDownloadData(for: duplicateData) { makeRunningTask() }

        let pendingCount = await context.pendingTasks.count
        #expect(pendingCount == 1)
    }

    @Test("creates state subject for pending items")
    func stateSubjectForPending() async {
        let context = makeContext(maxConcurrent: 1)

        await context.processDownloadData(for: makeDownloadData(id: "f1")) { makeRunningTask() }
        await context.processDownloadData(for: makeDownloadData(id: "f2")) { makeRunningTask() }

        let pendingId = AttachmentId(messageId: "msg", fileId: "f2")
        let hasState = await context.state[pendingId] != nil
        #expect(hasState)
    }

    // MARK: - handle

    @Test("terminal event removes task and starts next pending")
    func terminalEventStartsPending() async {
        let context = makeContext(maxConcurrent: 1)
        let id1 = AttachmentId(messageId: "msg", fileId: "f1")
        let secondStarted = AnyObjectHolder<Bool>()
        secondStarted.set(false)

        await context.processDownloadData(for: makeDownloadData(id: "f1")) { makeRunningTask() }

        await context.processDownloadData(for: makeDownloadData(id: "f2")) {
            secondStarted.set(true)
            return makeRunningTask()
        }

        #expect(secondStarted.get() == false)

        await context.handle(downloadEvent: .onComplete, attachmentId: id1)

        #expect(secondStarted.get() == true)
        let taskCount = await context.tasks.count
        let pendingCount = await context.pendingTasks.count
        #expect(taskCount == 1)
        #expect(pendingCount == 0)
    }

    @Test("failure event also frees slot")
    func failureEventFreesSlot() async {
        let context = makeContext(maxConcurrent: 1)
        let id1 = AttachmentId(messageId: "msg", fileId: "f1")
        let secondStarted = AnyObjectHolder<Bool>()
        secondStarted.set(false)

        await context.processDownloadData(for: makeDownloadData(id: "f1")) { makeRunningTask() }

        await context.processDownloadData(for: makeDownloadData(id: "f2")) {
            secondStarted.set(true)
            return makeRunningTask()
        }

        await context.handle(
            downloadEvent: .onFailure(NSError(domain: "test", code: 0)),
            attachmentId: id1
        )

        #expect(secondStarted.get() == true)
    }

    @Test("progress event does not free slot")
    func progressDoesNotFreeSlot() async {
        let context = makeContext(maxConcurrent: 1)
        let id1 = AttachmentId(messageId: "msg", fileId: "f1")
        let secondStarted = AnyObjectHolder<Bool>()
        secondStarted.set(false)

        await context.processDownloadData(for: makeDownloadData(id: "f1")) { makeRunningTask() }

        await context.processDownloadData(for: makeDownloadData(id: "f2")) {
            secondStarted.set(true)
            return makeRunningTask()
        }

        await context.handle(
            downloadEvent: .onProgress(.init(loaded: 50, total: 100)),
            attachmentId: id1
        )

        #expect(secondStarted.get() == false)
        let pendingCount = await context.pendingTasks.count
        #expect(pendingCount == 1)
    }

    @Test("handle emits event to state subject")
    func handleEmitsToState() async {
        let context = makeContext()
        let id = AttachmentId(messageId: "msg", fileId: "f1")

        await context.processDownloadData(for: makeDownloadData(id: "f1")) { makeRunningTask() }

        await context.handle(
            downloadEvent: .onProgress(.init(loaded: 50, total: 100)),
            attachmentId: id
        )

        let subject = await context.state[id]
        let value = subject?.value
        if case let .onProgress(progress) = value {
            #expect(progress.loaded == 50)
            #expect(progress.total == 100)
        } else {
            Issue.record("Expected onProgress event")
        }
    }

    // MARK: - cancelAll

    @Test("cancelAll clears active tasks and pending queue")
    func cancelAllClears() async {
        let context = makeContext(maxConcurrent: 1)

        await context.processDownloadData(for: makeDownloadData(id: "f1")) { makeRunningTask() }
        await context.processDownloadData(for: makeDownloadData(id: "f2")) { makeRunningTask() }
        await context.processDownloadData(for: makeDownloadData(id: "f3")) { makeRunningTask() }

        await context.cancelAll()

        let taskCount = await context.tasks.count
        let pendingCount = await context.pendingTasks.count
        #expect(taskCount == 0)
        #expect(pendingCount == 0)
    }

    // MARK: - subscribeState

    @Test("subscribeState creates subject if none exists")
    func subscribeStateCreatesSubject() async {
        let context = makeContext()
        let id = AttachmentId(messageId: "msg", fileId: "new")

        _ = await context.subscribeState(for: id)

        let hasState = await context.state[id] != nil
        #expect(hasState)
    }
}
