import Foundation
import SubstrateSdk
import SubstrateStorageSubscription
import CommonService

public final class AssetHubReQuoteService: ObservableSubscriptionSyncService<ObservableSubscriptionState> {
    override public func getRequests() throws -> [BatchStorageSubscriptionRequest] {
        let blockNumberRequest = BatchStorageSubscriptionRequest(
            innerRequest: UnkeyedSubscriptionRequest(
                storagePath: SystemPallet.blockNumberPath,
                localKey: ""
            ),
            mappingKey: nil
        )

        return [blockNumberRequest]
    }
}
