import Foundation
import AsyncExtensions
import Operation_iOS

actor MixnetUploadContext {
    struct Pending {
        let uploadData: MixnetUploadData
        let onUploadConfirm: @Sendable () -> Task<Void, Never>
    }

    var state = [AttachmentId: AsyncCurrentValueSubject<AttachmentProgressEvent?>]()
    var tasks: [AttachmentId: Task<Void, Never>] = [:]
    var pendingTasks: [Pending] = []

    let maxConcurrentUploads: Int
    let repository: AnyDataProviderRepository<Chat.AttachmentUploadingUpdate>
    let logger: LoggerProtocol

    init(
        repository: AnyDataProviderRepository<Chat.AttachmentUploadingUpdate>,
        maxConcurrentUploads: Int = 10,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.maxConcurrentUploads = maxConcurrentUploads
        self.repository = repository
        self.logger = logger
    }
}

private extension MixnetUploadContext {
    func updateMessageIfNeeded(uploadEvent: MixnetUploadEvent, attachmentId: AttachmentId) async throws {
        guard case let .onComplete(uploadingInfo) = uploadEvent else {
            return
        }

        let update = Chat.AttachmentUploadingUpdate(
            messageId: attachmentId.messageId,
            fileId: attachmentId.fileId,
            uploadingInfo: uploadingInfo
        )

        try await repository.saveOperation({ [update] }, { [] }).asyncExecute()
    }

    func updateState(uploadEvent: MixnetUploadEvent, attachmentId: AttachmentId) {
        if state[attachmentId] == nil {
            state[attachmentId] = AsyncCurrentValueSubject(nil)
        }

        state[attachmentId]?.send(uploadEvent.toAttachmentProgressEvent())
    }

    func startNextPendingIfNeeded() {
        while tasks.count < maxConcurrentUploads, !pendingTasks.isEmpty {
            let pending = pendingTasks.removeFirst()
            let id = pending.uploadData.attachmentId
            tasks[id] = pending.onUploadConfirm()
        }
    }
}

extension MixnetUploadContext {
    func processUploadData(
        for uploadData: MixnetUploadData,
        onUploadConfirm: @escaping @Sendable () -> Task<Void, Never>
    ) {
        let id = uploadData.attachmentId

        guard
            tasks[id] == nil,
            !pendingTasks.contains(where: { $0.uploadData.attachmentId == id })
        else {
            return
        }

        if state[id] == nil {
            state[id] = AsyncCurrentValueSubject<AttachmentProgressEvent?>(nil)
        }

        if tasks.count < maxConcurrentUploads {
            tasks[id] = onUploadConfirm()
        } else {
            pendingTasks.append(Pending(
                uploadData: uploadData,
                onUploadConfirm: onUploadConfirm
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

    func handle(uploadEvent: MixnetUploadEvent, attachmentId: AttachmentId) async {
        do {
            try await updateMessageIfNeeded(uploadEvent: uploadEvent, attachmentId: attachmentId)
            updateState(uploadEvent: uploadEvent, attachmentId: attachmentId)
        } catch {
            logger.error("Failed to handle event: \(error)")
        }

        switch uploadEvent {
        case .onComplete,
             .onFailure:
            tasks[attachmentId] = nil
            startNextPendingIfNeeded()
        case .onProgress:
            break
        }
    }
}
