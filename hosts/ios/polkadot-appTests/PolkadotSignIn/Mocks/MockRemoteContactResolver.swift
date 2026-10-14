@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

struct MockRemoteContactResolver: RemoteContactResolving {
    let result: Result<Chat.RemoteContact?, Error>

    func fetch(by _: AccountId) async throws -> Chat.RemoteContact? {
        try result.get()
    }
}
