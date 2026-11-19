import Foundation

/// One notification seq of a period, claimed through one of the person's membership collections.
public struct NotificationSlot {
    public let personOrigin: PersonOrigin
    public let period: UInt32
    public let seq: UInt8

    public init(personOrigin: PersonOrigin, period: UInt32, seq: UInt8) {
        self.personOrigin = personOrigin
        self.period = period
        self.seq = seq
    }

    var key: Key {
        Key(collection: personOrigin.collectionIdentifier, period: period, seq: seq)
    }
}

extension NotificationSlot {
    /// What tells two slots apart: the same seq of the same period in another collection is another slot.
    struct Key: Hashable, Sendable {
        let collection: MembersPallet.CollectionIdentifier
        let period: UInt32
        let seq: UInt8
    }
}
