import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

extension Chat {
    struct AttachmentUploadingUpdate {
        let messageId: Chat.MessageId
        let fileId: String
        let uploadingInfo: Chat.LocalMessage.Content.FileUploadingInfo
    }
}

final class AttachmentUploadingMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.messageId)
    }

    typealias DataProviderModel = Chat.AttachmentUploadingUpdate
    typealias CoreDataEntity = CDChatMessage
}

extension AttachmentUploadingMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingMessage
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        guard entity.messageId != nil else {
            throw MappingError.missingMessage
        }

        let content = try ChatMessageEntityMapper.getContent(from: entity)

        guard
            case let .richText(richText) = content,
            let attachments = richText.attachments else {
            return
        }

        let newAttachments: [Chat.LocalMessage.Content.Attachment] = attachments.map { attachment in
            switch attachment {
            case let .localUploadable(uploadableFile) where uploadableFile.relativeLocalPath == model.fileId:
                let newUploadableFile = Chat.LocalMessage.Content.LocalUploadableFile(
                    relativeLocalPath: uploadableFile.relativeLocalPath,
                    meta: uploadableFile.meta,
                    uploadingInfo: model.uploadingInfo
                )

                return .localUploadable(newUploadableFile)
            default:
                return attachment
            }
        }

        let newContent: Chat.LocalMessage.Content = .richText(
            .init(
                text: richText.text,
                attachments: newAttachments
            )
        )

        let contentEntity = entity.content
        contentEntity?.data = try newContent.scaleEncoded()

        // trigger change of the message
        entity.content = contentEntity
        entity.markModified()
        entity.touchParent()
    }
}

extension Chat.AttachmentUploadingUpdate: Identifiable {
    var identifier: String {
        messageId
    }
}
