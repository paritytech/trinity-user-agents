import Foundation

struct PeerConnectionSignalSendResult {
    let isFullySent: Bool

    static let fullySent = PeerConnectionSignalSendResult(isFullySent: true)
    static let notFullySent = PeerConnectionSignalSendResult(isFullySent: false)
}
