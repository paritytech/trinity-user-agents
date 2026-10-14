import Foundation

struct MixnetUploadData {
    let fileMeta: ChatRemoteMessageContent.FileMeta
    let attachmentId: AttachmentId
    let chatId: Chat.Id
}

enum MixnetUploadList {
    static func createUploadList(from message: Chat.LocalMessage) -> [MixnetUploadData] {
        guard message.status.isOutgoing, case let .richText(content) = message.content else {
            return []
        }

        return content.attachments?.compactMap { attachment in
            switch attachment {
            case let .localUploadable(localFile) where localFile.uploadingInfo == nil:
                MixnetUploadData(
                    fileMeta: localFile.meta,
                    attachmentId: AttachmentId(
                        messageId: message.messageId,
                        fileId: localFile.relativeLocalPath
                    ),
                    chatId: message.chatId
                )
            default:
                nil
            }
        } ?? []
    }
}
