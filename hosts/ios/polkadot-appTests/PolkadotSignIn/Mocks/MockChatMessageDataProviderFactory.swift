@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockChatMessageDataProviderFactory: ChatMessageDataProviderMaking {
    func createNewRemoteMessagesLifecycleProvider() -> StreamableProvider<Chat.LocalMessage> {
        fatalError("Not used in device sync tests")
    }

    func subscribeMessagesSnapshot(
        with _: NSPredicate?,
        deliverOn _: DispatchQueue,
        update _: @escaping ([Chat.LocalMessage]) -> Void
    ) -> AnyObject {
        MockSnapshotSubscription()
    }
}
