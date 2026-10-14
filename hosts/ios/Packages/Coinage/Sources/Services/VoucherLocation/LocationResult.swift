import Foundation
import SubstrateSdk
import SubstrateStorageSubscription
import Individuality

struct MemberStatusResult: BatchStorageSubscriptionResult {
    struct MemberUpdate {
        let publicKey: PublicKey
        let ringPosition: MembersPallet.RingPosition?
    }

    /// Ring state is shared, so these arrive per ring rather than per voucher.
    struct RingStatusUpdate {
        let recycler: RecyclerKey
        let ringKeysStatus: MembersPallet.RingKeysStatus?
    }

    /// `RecyclersUnloadedCount` for a ring. The entry is an `OptionQuery` populated only as aliases
    /// are unloaded, so an absent reading means "none yet", not "unknown".
    struct UnloadedCountUpdate {
        let recycler: RecyclerKey
        let unloadedCount: UInt32?
    }

    let ringPositionUpdates: [MemberUpdate]
    let ringStatusUpdates: [RingStatusUpdate]
    let unloadedCountUpdates: [UnloadedCountUpdate]
    let blockHash: BlockHashData?

    init(
        values: [BatchStorageSubscriptionResultValue],
        blockHashJson: JSON,
        context: [CodingUserInfoKey: Any]?
    ) throws {
        var updates: [MemberUpdate] = []
        var ringStatusUpdates: [RingStatusUpdate] = []
        var unloadedCountUpdates: [UnloadedCountUpdate] = []

        for item in values {
            guard
                let mappingKey = item.mappingKey,
                let subKey = SubscriptionKey(mappingKey: mappingKey)
            else {
                continue
            }

            switch subKey {
            case let .member(publicKey):
                let ringPosition = try? item.value.map(
                    to: MembersPallet.RingPosition?.self,
                    with: context
                )

                updates.append(.init(
                    publicKey: publicKey,
                    ringPosition: ringPosition
                ))

            case let .ringStatus(recycler):
                let ringKeysStatus = try? item.value.map(
                    to: MembersPallet.RingKeysStatus?.self,
                    with: context
                )

                ringStatusUpdates.append(.init(
                    recycler: recycler,
                    ringKeysStatus: ringKeysStatus
                ))

            case let .unloadedCount(recycler):
                let count = try? item.value.map(
                    to: StringScaleMapper<UInt32>?.self,
                    with: context
                )

                unloadedCountUpdates.append(.init(
                    recycler: recycler,
                    unloadedCount: count?.value
                ))
            }
        }

        ringPositionUpdates = updates
        self.ringStatusUpdates = ringStatusUpdates
        self.unloadedCountUpdates = unloadedCountUpdates
        blockHash = try blockHashJson.map(to: BlockHashData?.self, with: context)
    }
}
