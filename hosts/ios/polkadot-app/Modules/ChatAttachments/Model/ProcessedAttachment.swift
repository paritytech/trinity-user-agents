import Foundation

struct ProcessedAttachment: Equatable {
    let fileId: String
    let fileUrl: URL
    let meta: ChatRemoteMessageContent.FileMeta

    func toMessageAttachment() -> Chat.LocalMessage.Content.Attachment {
        .localUploadable(
            .init(
                relativeLocalPath: fileId,
                meta: meta,
                uploadingInfo: nil
            )
        )
    }
}
