@testable import PolkadotUI

final class SpyChatInputHandler: ChatInputHandling {
    private(set) var changeCount = 0

    func chatInputDidSend(_: String) {}
    func chatInputDidTransfer() {}
    func chatInputDidAttachment() {}
    func chatInputDidCancelReply() {}
    func chatInputDidCancelEdit() {}

    func chatInputDidChange() {
        changeCount += 1
    }
}
