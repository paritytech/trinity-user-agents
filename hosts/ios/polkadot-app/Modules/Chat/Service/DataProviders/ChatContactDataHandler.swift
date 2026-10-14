import Foundation
import Operation_iOS

protocol ChatContactDataHandling: AnyObject {
    func handleChatContacts(result: Result<[DataProviderChange<Chat.Contact>], Error>)
}
