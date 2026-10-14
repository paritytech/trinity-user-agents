import UIKit
import Operation_iOS
import Foundation_iOS

final class ChatAttachmentsInteractor {
    weak var presenter: ChatAttachmentsInteractorOutputProtocol?

    let providers: [ChatAttachmentProviding]
    let uploadStore: AttachmentStoring
    let audioSessionManager: AudioSessionManaging
    let logger: LoggerProtocol

    private var preparationTask: Task<Void, Never>?

    init(
        providers: [ChatAttachmentProviding],
        uploadStore: AttachmentStoring,
        audioSessionManager: AudioSessionManaging,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.providers = providers
        self.uploadStore = uploadStore
        self.audioSessionManager = audioSessionManager
        self.logger = logger
    }
}

private extension ChatAttachmentsInteractor {
    func configureAudioSession() {
        do {
            let activities = Set(providers.compactMap(\.neededAudioActivity))

            guard !activities.isEmpty else {
                return
            }

            try audioSessionManager.registerActivities(activities, for: self)
        } catch {
            logger.error("Failed to set up audio session: \(error)")
        }
    }

    func completeAudioSession() {
        do {
            try audioSessionManager.deregisterActivities(for: self)
        } catch {
            logger.error("Failed to complete audio session: \(error)")
        }
    }

    func prepareProviders() {
        preparationTask = Task { [providers, uploadStore, weak self] in
            do {
                var attachments: [ProcessedAttachment] = []

                for provider in providers {
                    let attachment = try await provider.prepareForSend(using: uploadStore)
                    self?.logger.debug("Attachment prepared: \(attachment.meta.fileSize) \(attachment.meta.mimeType)")
                    attachments.append(attachment)
                }

                await self?.presenter?.didProcessAttachments(attachments)
            } catch {
                guard !Task.isCancelled else {
                    self?.logger.debug("Preparation task is cancelled")
                    return
                }

                self?.logger.error("Preview preparation failed: \(error)")
            }
        }
    }
}

extension ChatAttachmentsInteractor: ChatAttachmentsInteractorInputProtocol {
    func setup() {
        configureAudioSession()
        prepareProviders()
    }

    func complete(rejectingAttachments: [ProcessedAttachment]) {
        preparationTask?.cancel()
        preparationTask = nil

        completeAudioSession()

        rejectingAttachments.forEach { attachment in
            do {
                try uploadStore.remove(for: attachment.fileId)
                logger.debug("Removed attachment: \(attachment.fileId)")
            } catch {
                logger.error("Can't remove file: \(attachment.fileId). Skipping...")
            }
        }
    }
}
