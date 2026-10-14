import Observation
import SwiftUI
import Foundation

public struct ChatRequestListItem: Identifiable, Equatable {
    public let id: String
    public let contactName: String
    public let messageText: String
    public let avatarViewModel: AvatarViewModel
    public let date: Date
    public let isSeen: Bool

    public init(
        id: String,
        contactName: String,
        avatarViewModel: AvatarViewModel,
        messageText: String,
        date: Date,
        isSeen: Bool
    ) {
        self.id = id
        self.contactName = contactName
        self.avatarViewModel = avatarViewModel
        self.messageText = messageText
        self.date = date
        self.isSeen = isSeen
    }
}

public protocol ChatRequestListViewModelProtocol {
    var items: [ChatRequestListItem] { get }
    var onItemSelection: ((String) -> Void)? { get set }
}

@Observable
public class ChatRequestListViewModel: ChatRequestListViewModelProtocol {
    public let autoupdateInterval: TimeInterval
    public var items: [ChatRequestListItem] = []
    public var onItemSelection: ((String) -> Void)?

    public init(autoupdateInterval: TimeInterval = 60) {
        self.autoupdateInterval = autoupdateInterval
    }
}
