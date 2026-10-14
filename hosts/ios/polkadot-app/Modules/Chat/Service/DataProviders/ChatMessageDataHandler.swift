import Foundation
import Operation_iOS

protocol ChatMessageDataHandling: AnyObject {
    func handleChatMessages(result: Result<[DataProviderChange<Chat.LocalMessage>], Error>)
}
