import Foundation
import SubstrateSdk

enum ChatOpenModel {
    struct NewRequest {
        let remoteContact: Chat.RemoteContact
        let ownKeyId: Chat.Contact.Own
    }

    case existingChat(Chat.Id)
    case newRequest(NewRequest)
}
