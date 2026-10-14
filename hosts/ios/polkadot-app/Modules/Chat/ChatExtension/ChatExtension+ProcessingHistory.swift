import Foundation

extension ChatExtension {
    enum ProcessingHistoryOutcome: Int16 {
        case firstEncounter
        case previouslyProcessed
    }

    struct ProcessingHistory {
        let messageId: String
        let chatId: String
        let extensionId: String
    }
}
