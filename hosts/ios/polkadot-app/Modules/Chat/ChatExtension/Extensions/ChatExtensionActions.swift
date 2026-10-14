import Foundation

enum ChatExtensionActions {
    struct FAQ {
        let id: ChatExtension.ActionId
        let question: String
        let answer: String
    }

    struct ActionModel {
        let title: String
        let subtitle: String
        let identifier: ChatExtension.Id
    }
}
