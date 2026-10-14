import UIKit

public protocol ChatInputHandling: AnyObject {
    func chatInputDidSend(_ text: String)
    func chatInputDidTransfer()
    func chatInputDidAttachment()
    func chatInputDidCancelReply()
    func chatInputDidCancelEdit()
    func chatInputDidChange()
}
