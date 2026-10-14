import UIKit
import Foundation
import PolkadotUI
import UniformTypeIdentifiers
import Kingfisher
import Foundation_iOS

protocol ChatAttachmentViewModelMaking {
    func makeAttachmentItem(
        for attachment: Chat.LocalMessage.Content.Attachment,
        messageId: Chat.MessageId,
        onSelection: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem?
}

final class ChatAttachmentViewModelFactory {
    static var maxThumbnailSize: CGFloat {
        240 * UIScreen.main.scale
    }

    let uploadAttachmentStore: AttachmentStoring
    let attachmentUploadStateProvider: AttachmentLoadProgressProvidable
    let downloadAttachmentStore: AttachmentStoring
    let attachmentDownloadStateProvider: AttachmentLoadProgressProvidable
    let durationFormatter: TimeFormatterProtocol

    init(
        uploadAttachmentStore: AttachmentStoring,
        attachmentUploadStateProvider: AttachmentLoadProgressProvidable,
        downloadAttachmentStore: AttachmentStoring,
        attachmentDownloadStateProvider: AttachmentLoadProgressProvidable,
        durationFormatter: TimeFormatterProtocol = MinuteSecondFormatter()
    ) {
        self.uploadAttachmentStore = uploadAttachmentStore
        self.attachmentUploadStateProvider = attachmentUploadStateProvider
        self.downloadAttachmentStore = downloadAttachmentStore
        self.attachmentDownloadStateProvider = attachmentDownloadStateProvider
        self.durationFormatter = durationFormatter
    }
}

// MARK: - Private factory helpers

private extension ChatAttachmentViewModelFactory {
    struct ButtonConfigurations {
        let success: ChatMessageMediaViewConfiguration.ButtonConfiguration?
        let failure: ChatMessageMediaViewConfiguration.ButtonConfiguration?
        let loading: ChatMessageMediaViewConfiguration.ButtonConfiguration?

        init(
            success: ChatMessageMediaViewConfiguration.ButtonConfiguration?,
            failure: ChatMessageMediaViewConfiguration.ButtonConfiguration?,
            loading: ChatMessageMediaViewConfiguration.ButtonConfiguration? = .init(
                style: .loading(cancelable: false)
            )
        ) {
            self.success = success
            self.failure = failure
            self.loading = loading
        }
    }

    func progressDrivenOverlayInfoProvider(
        needsLoading: Bool,
        service: AttachmentLoadProgressProvidable,
        attachmentId: AttachmentId,
        loadingDirection: LoadingDirection,
        successConfiguration: ChatMessageOverlayInfoViewConfiguration? = nil
    ) -> (any ChatMessageOverlayInfoProviding)? {
        guard needsLoading else {
            return successConfiguration.map { StaticChatMessageOverlayInfoProvider($0) }
        }
        let progressViewModel = AttachmentProgressViewModel(
            service: service,
            attachmentId: attachmentId,
            loadingDirection: loadingDirection
        )
        switch loadingDirection {
        case .upload:
            return ProgressDrivenOverlayInfoProvider.forUpload(
                progressViewModel: progressViewModel,
                successConfiguration: successConfiguration
            )
        case .download:
            return ProgressDrivenOverlayInfoProvider.forDownload(
                progressViewModel: progressViewModel,
                successConfiguration: successConfiguration
            )
        }
    }

    func progressDrivenButtonConfigurationProvider(
        needsLoading: Bool,
        service: AttachmentLoadProgressProvidable,
        attachmentId: AttachmentId,
        loadingDirection: LoadingDirection,
        configurations: ButtonConfigurations
    ) -> (any ChatMessageMediaButtonConfigurationProviding)? {
        guard needsLoading else {
            return configurations.success.map { StaticChatMessageMediaButtonConfigurationProvider($0) }
        }
        let progressViewModel = AttachmentProgressViewModel(
            service: service,
            attachmentId: attachmentId,
            loadingDirection: loadingDirection
        )
        return ProgressDrivenButtonConfigurationProvider(
            progressViewModel: progressViewModel,
            successConfiguration: configurations.success,
            failureConfiguration: configurations.failure,
            loadingConfiguration: configurations.loading,
            emitProgressOnSubscription: true
        )
    }

    func downloadFailureOverlayProvider(
        needsLoading: Bool,
        attachmentId: AttachmentId
    ) -> (any ChatMessageMediaFailureOverlayProviding)? {
        guard needsLoading else { return nil }

        return DownloadFailureOverlayProvider(
            progressViewModel: AttachmentProgressViewModel(
                service: attachmentDownloadStateProvider,
                attachmentId: attachmentId,
                loadingDirection: .download
            )
        )
    }

    func remotePreviewProvider(
        thumbnail: Data?,
        attachmentId: AttachmentId,
        filename: String,
        innerProvider: ImageDataProvider
    ) -> BlurHashMediaPreviewProvider {
        let hasFile = { [downloadAttachmentStore] in
            downloadAttachmentStore.hasFile(for: filename)
        }
        let imageViewModel = LocalImageViewModel(
            provider: LoadableAttachmentImageProvider(
                attachmentId: attachmentId,
                innerProvider: innerProvider,
                service: attachmentDownloadStateProvider,
                dataExistencePredicate: hasFile
            ),
            keepsCurrentImageWhileLoading: true
        )
        return BlurHashMediaPreviewProvider(
            thumbnail: thumbnail,
            dataExistencePredicate: hasFile,
            mediaProvider: imageViewModel
        )
    }

    func createFromLocalUploadable(
        file: Chat.LocalMessage.Content.LocalUploadableFile,
        messageId: Chat.MessageId,
        onSelection: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem? {
        switch file.meta {
        case .image:
            let fileUrl = uploadAttachmentStore.fileURL(for: file.relativeLocalPath)

            let attachmentId = AttachmentId(
                messageId: messageId,
                fileId: file.relativeLocalPath
            )

            let imageViewModel = LocalImageViewModel(
                provider: ThumbnailImageDataProvider(
                    url: fileUrl,
                    maxPixelSize: Self.maxThumbnailSize
                ),
                keepsCurrentImageWhileLoading: false
            )

            let buttonConfigurationProvider = progressDrivenButtonConfigurationProvider(
                needsLoading: file.uploadingInfo == nil,
                service: attachmentUploadStateProvider,
                attachmentId: attachmentId,
                loadingDirection: .upload,
                configurations: .init(success: nil, failure: nil)
            )

            let topLeadingInfoProvider = progressDrivenOverlayInfoProvider(
                needsLoading: file.uploadingInfo == nil,
                service: attachmentUploadStateProvider,
                attachmentId: attachmentId,
                loadingDirection: .upload
            )

            // Corners are overridden later by the outer bubble composition based on tail side.
            let mediaConfiguration = ChatMessageMediaViewConfiguration(
                previewProvider: imageViewModel,
                topLeadingInfoProvider: topLeadingInfoProvider,
                buttonConfigurationProvider: buttonConfigurationProvider,
                tapOnMedia: onSelection
            )

            return ChatRichTextMessageConfiguration.AttachmentItem(
                identifier: attachmentId.stringValue,
                mediaConfiguration: mediaConfiguration
            )

        case let .video(videoMeta):
            let fileUrl = uploadAttachmentStore.fileURL(for: file.relativeLocalPath)

            let attachmentId = AttachmentId(
                messageId: messageId,
                fileId: file.relativeLocalPath
            )

            let imageViewModel = LocalImageViewModel(
                provider: AVAssetImageDataProvider(assetURL: fileUrl, time: .zero),
                keepsCurrentImageWhileLoading: false
            )

            let duration = try? durationFormatter.string(from: TimeInterval(videoMeta.duration))
            let durationConfiguration: ChatMessageOverlayInfoViewConfiguration? = duration.map {
                .init(icon: nil, title: $0, backgroundColor: UIColor(resource: .black45))
            }

            let buttonConfigurationProvider = progressDrivenButtonConfigurationProvider(
                needsLoading: file.uploadingInfo == nil,
                service: attachmentUploadStateProvider,
                attachmentId: attachmentId,
                loadingDirection: .upload,
                configurations: .init(
                    success: .init(style: .play, action: onSelection),
                    failure: nil
                )
            )

            let topLeadingInfoProvider = progressDrivenOverlayInfoProvider(
                needsLoading: file.uploadingInfo == nil,
                service: attachmentUploadStateProvider,
                attachmentId: attachmentId,
                loadingDirection: .upload,
                successConfiguration: durationConfiguration
            )

            // Corners are overridden later by the outer bubble composition based on tail side.
            let mediaConfiguration = ChatMessageMediaViewConfiguration(
                previewProvider: imageViewModel,
                topLeadingInfoProvider: topLeadingInfoProvider,
                buttonConfigurationProvider: buttonConfigurationProvider,
                tapOnMedia: onSelection
            )

            return ChatRichTextMessageConfiguration.AttachmentItem(
                identifier: attachmentId.stringValue,
                mediaConfiguration: mediaConfiguration
            )

        case .general:
            return nil
        }
    }

    func createFromRemoteDownloadable(
        variant: ChatRemoteMessageContent.FileVariant,
        messageId: Chat.MessageId,
        onSelection: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem? {
        let filename = variant.filename

        switch variant.meta {
        case let .image(imageMeta):
            return makeImageItem(
                meta: imageMeta,
                filename: filename,
                messageId: messageId,
                onSelection: onSelection
            )
        case let .video(videoMeta):
            return makeVideoItem(
                meta: videoMeta,
                filename: filename,
                messageId: messageId,
                onSelection: onSelection
            )
        case .general:
            return nil
        }
    }

    func makeImageItem(
        meta: ChatRemoteMessageContent.ImageFileMeta,
        filename: String,
        messageId: Chat.MessageId,
        onSelection: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem {
        let attachmentId = AttachmentId(messageId: messageId, fileId: filename)
        let needsLoading = !downloadAttachmentStore.hasFile(for: filename)
        let previewProvider = remotePreviewProvider(
            thumbnail: meta.thumbnail,
            attachmentId: attachmentId,
            filename: filename,
            innerProvider: ThumbnailImageDataProvider(
                url: downloadAttachmentStore.fileURL(for: filename),
                maxPixelSize: Self.maxThumbnailSize
            )
        )
        let infoProvider = progressDrivenOverlayInfoProvider(
            needsLoading: needsLoading,
            service: attachmentDownloadStateProvider,
            attachmentId: attachmentId,
            loadingDirection: .download
        )

        // Corners are overridden later by the outer bubble composition based on tail side.
        let mediaConfiguration = ChatMessageMediaViewConfiguration(
            previewProvider: previewProvider,
            previewBackgroundColor: .bgSurfaceNested,
            topLeadingInfoProvider: infoProvider,
            failureOverlayProvider: downloadFailureOverlayProvider(
                needsLoading: needsLoading,
                attachmentId: attachmentId
            ),
            tapOnMedia: onSelection
        )
        return .init(identifier: attachmentId.stringValue, mediaConfiguration: mediaConfiguration)
    }

    func makeVideoItem(
        meta: ChatRemoteMessageContent.VideoFileMeta,
        filename: String,
        messageId: Chat.MessageId,
        onSelection: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem {
        let attachmentId = AttachmentId(messageId: messageId, fileId: filename)
        let needsLoading = !downloadAttachmentStore.hasFile(for: filename)
        let previewProvider = remotePreviewProvider(
            thumbnail: meta.thumbnail,
            attachmentId: attachmentId,
            filename: filename,
            innerProvider: AVAssetImageDataProvider(
                assetURL: downloadAttachmentStore.fileURL(for: filename),
                time: .zero
            )
        )
        let duration = try? durationFormatter.string(from: TimeInterval(meta.duration))
        let durationConfiguration = duration.map {
            ChatMessageOverlayInfoViewConfiguration(
                icon: nil,
                title: $0,
                backgroundColor: UIColor(resource: .black45)
            )
        }
        let buttonProvider = progressDrivenButtonConfigurationProvider(
            needsLoading: needsLoading,
            service: attachmentDownloadStateProvider,
            attachmentId: attachmentId,
            loadingDirection: .download,
            configurations: .init(
                success: .init(style: .play, action: onSelection),
                failure: nil,
                loading: nil
            )
        )
        let infoProvider = progressDrivenOverlayInfoProvider(
            needsLoading: needsLoading,
            service: attachmentDownloadStateProvider,
            attachmentId: attachmentId,
            loadingDirection: .download,
            successConfiguration: durationConfiguration
        )

        // Corners are overridden later by the outer bubble composition based on tail side.
        let mediaConfiguration = ChatMessageMediaViewConfiguration(
            previewProvider: previewProvider,
            previewBackgroundColor: .bgSurfaceNested,
            topLeadingInfoProvider: infoProvider,
            buttonConfigurationProvider: buttonProvider,
            failureOverlayProvider: downloadFailureOverlayProvider(
                needsLoading: needsLoading,
                attachmentId: attachmentId
            ),
            tapOnMedia: onSelection
        )
        return .init(identifier: attachmentId.stringValue, mediaConfiguration: mediaConfiguration)
    }
}

extension ChatAttachmentViewModelFactory: ChatAttachmentViewModelMaking {
    func makeAttachmentItem(
        for attachment: Chat.LocalMessage.Content.Attachment,
        messageId: Chat.MessageId,
        onSelection: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem? {
        switch attachment {
        case let .localUploadable(file):
            createFromLocalUploadable(
                file: file,
                messageId: messageId,
                onSelection: onSelection
            )

        case let .remoteDownloadable(fileVariant):
            createFromRemoteDownloadable(
                variant: fileVariant,
                messageId: messageId,
                onSelection: onSelection
            )
        }
    }
}
