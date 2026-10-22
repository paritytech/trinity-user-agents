import AsyncExtensions
import Foundation

/// The upload or the download state of the photos and the videos of the rooms of the chat product, as the media
/// views of the native chat read it (`AttachmentLoadProgressProvidable`).
actor ProductMediaProgress: AttachmentLoadProgressProvidable {
    private var states: [AttachmentId: AsyncCurrentValueSubject<AttachmentProgressEvent?>] = [:]

    func subscribeState(for attachmentId: AttachmentId) -> AnyAsyncSequence<AttachmentProgressEvent?> {
        subject(for: attachmentId).eraseToAnyAsyncSequence()
    }

    func report(_ event: AttachmentProgressEvent?, for attachmentId: AttachmentId) {
        subject(for: attachmentId).send(event)
    }

    private func subject(for attachmentId: AttachmentId) -> AsyncCurrentValueSubject<AttachmentProgressEvent?> {
        if let subject = states[attachmentId] {
            return subject
        }

        let subject = AsyncCurrentValueSubject<AttachmentProgressEvent?>(nil)
        states[attachmentId] = subject
        return subject
    }
}
