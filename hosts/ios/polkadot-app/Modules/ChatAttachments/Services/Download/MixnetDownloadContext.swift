import Foundation
import AsyncExtensions

actor MixnetDownloadContext {
    struct Pending {
        let downloadData: MixnetDownloadData
        let onDownloadConfirm: @Sendable () -> Task<Void, Never>
    }

    var state = [AttachmentId: AsyncCurrentValueSubject<AttachmentProgressEvent?>]()
    var tasks: [AttachmentId: Task<Void, Never>] = [:]
    var pendingTasks: [Pending] = []

    let maxConcurrentDownloads: Int
    let logger: LoggerProtocol

    init(
        maxConcurrentDownloads: Int = 10,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.maxConcurrentDownloads = maxConcurrentDownloads
        self.logger = logger
    }
}

extension MixnetDownloadContext {
    func processDownloadData(
        for downloadData: MixnetDownloadData,
        onDownloadConfirm: @escaping @Sendable () -> Task<Void, Never>
    ) {
        let id = downloadData.attachmentId

        guard
            tasks[id] == nil,
            !pendingTasks.contains(where: { $0.downloadData.attachmentId == id })
        else {
            return
        }

        if state[id] == nil {
            state[id] = AsyncCurrentValueSubject<AttachmentProgressEvent?>(nil)
        }

        if tasks.count < maxConcurrentDownloads {
            tasks[id] = onDownloadConfirm()
        } else {
            pendingTasks.append(Pending(
                downloadData: downloadData,
                onDownloadConfirm: onDownloadConfirm
            ))
        }
    }

    func cancelAll() {
        tasks.values.forEach { $0.cancel() }
        tasks.removeAll()
        pendingTasks.removeAll()
    }

    func subscribeState(for attachmentId: AttachmentId) -> AnyAsyncSequence<AttachmentProgressEvent?> {
        if let subject = state[attachmentId] {
            return subject.eraseToAnyAsyncSequence()
        }

        let subject = AsyncCurrentValueSubject<AttachmentProgressEvent?>(nil)
        state[attachmentId] = subject

        return subject.eraseToAnyAsyncSequence()
    }

    func handle(downloadEvent: AttachmentProgressEvent, attachmentId: AttachmentId) {
        if state[attachmentId] == nil {
            state[attachmentId] = AsyncCurrentValueSubject(nil)
        }

        state[attachmentId]?.send(downloadEvent)

        switch downloadEvent {
        case .onComplete,
             .onFailure:
            tasks[attachmentId] = nil
            startNextPendingIfNeeded()
        case .onProgress:
            break
        }
    }
}

private extension MixnetDownloadContext {
    func startNextPendingIfNeeded() {
        while tasks.count < maxConcurrentDownloads, !pendingTasks.isEmpty {
            let pending = pendingTasks.removeFirst()
            let id = pending.downloadData.attachmentId
            tasks[id] = pending.onDownloadConfirm()
        }
    }
}
