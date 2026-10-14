import Foundation

protocol SSORequestHandling<Message> {
    associatedtype Message
    func canHandle(_ message: Message) -> Bool

    func handle(
        message: Message,
        from host: PolkadotSignInHost
    ) async
}
