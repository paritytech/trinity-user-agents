import UIKit
import Foundation
import SubstrateSdk

protocol ChatCallPresentationManaging {
    func presentCall(
        with peer: CallPeer,
        using engine: CallEngineProtocol,
        role: CallRole,
        callType: ChatCallType
    )
}

final class ChatChatCallPresentationManager {
    let logger: LoggerProtocol

    init(logger: LoggerProtocol) {
        self.logger = logger
    }
}

extension ChatChatCallPresentationManager: ChatCallPresentationManaging {
    func presentCall(
        with peer: CallPeer,
        using engine: CallEngineProtocol,
        role: CallRole,
        callType: ChatCallType
    ) {
        DispatchQueue.main.async {
            guard
                let callView = ChatCallViewFactory.createView(
                    peer: peer,
                    engine: engine,
                    role: role,
                    callType: callType
                ) else {
                self.logger.error("Can't create chat call")
                return
            }

            let rootController = UIWindow.keyWindow?.rootViewController

            callView.controller.modalPresentationStyle = .fullScreen
            callView.controller.modalTransitionStyle = .crossDissolve
            rootController?.present(callView.controller, animated: true)
        }
    }
}
