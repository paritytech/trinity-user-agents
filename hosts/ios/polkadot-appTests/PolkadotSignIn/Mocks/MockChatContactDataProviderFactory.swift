@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockChatContactDataProviderFactory: ChatContactDataProviderMaking {
    func createAllContactsProvider() -> StreamableProvider<Chat.Contact> {
        fatalError("Not used in device sync tests")
    }

    func subscribeContactsSnapshot(
        for _: NSPredicate?,
        deliverOn _: DispatchQueue,
        update _: @escaping ([Chat.Contact]) -> Void,
        failure _: @escaping (Error) -> Void
    ) -> AnyObject {
        MockSnapshotSubscription()
    }

    func subscribeChatsSnapshot(
        for _: NSPredicate?,
        deliverOn _: DispatchQueue,
        update _: @escaping ([Chat.LocalModel]) -> Void,
        failure _: @escaping (Error) -> Void
    ) -> AnyObject {
        MockSnapshotSubscription()
    }
}
