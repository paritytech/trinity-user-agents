import Foundation
import SubstrateSdk
import Individuality

/// Key of `Coinage.RecyclersUnloadedCount`: `(InstanceId, Denomination, RingIndex)`.
///
/// Unlike `RecyclerAliasStates`, this is a plain map whose *single* key happens to be a tuple, hashed
/// once as a whole. That is why it encodes as one unkeyed container rather than going through the
/// n-map path — the runtime declares one hasher, so the SDK keeps the tuple intact.
struct RecyclerUnloadedCountKey: Encodable {
    let instanceId: CoinageInstanceId
    let exponent: Int16
    let ringIndex: MembersPallet.RingIndex

    func encode(to encoder: Encoder) throws {
        var container = encoder.unkeyedContainer()

        try container.encode(StringCodable(wrappedValue: instanceId))
        try container.encode(StringCodable(wrappedValue: exponent))
        try container.encode(StringCodable(wrappedValue: ringIndex))
    }
}
