import Foundation
import SubstrateSdk

enum ChatOpenModel {
    /// A chat by its id. A person chat id opens the chat with that account in the chat product.
    case existingChat(Chat.Id)
}
