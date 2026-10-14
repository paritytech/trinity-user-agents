import Foundation
import SubstrateSdk

extension ChatMessageEntityMapper {
    enum MessageState {
        case newMessage
        case existingMessage

        var isNew: Bool {
            switch self {
            case .newMessage:
                true
            case .existingMessage:
                false
            }
        }
    }

    func ensureValidMessage(
        entity: CoreDataEntity,
        from model: DataProviderModel
    ) -> MessageState? {
        guard entity.messageId != nil else {
            return .newMessage
        }

        // outgoing message can't be changed to incoming and vice versa
        guard
            let entityStatus = Chat.LocalMessage.Status(rawValue: entity.status),
            entityStatus.statusClass == model.status.statusClass else {
            return nil
        }

        return .existingMessage
    }
}
