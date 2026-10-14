import Foundation

struct MixnetDownloadData {
    let fileVariant: ChatRemoteMessageContent.FileVariant
    let attachmentId: AttachmentId
}

enum MixnetDownloadList {
    static func createDownloadList(from message: Chat.LocalMessage) -> [MixnetDownloadData] {
        guard message.status.isIncoming, case let .richText(content) = message.content else {
            return []
        }

        return content.attachments?.compactMap { attachment in
            switch attachment {
            case let .remoteDownloadable(fileVariant):
                MixnetDownloadData(
                    fileVariant: fileVariant,
                    attachmentId: AttachmentId(
                        messageId: message.messageId,
                        fileId: fileVariant.filename
                    )
                )
            case .localUploadable:
                nil
            }
        } ?? []
    }
}
