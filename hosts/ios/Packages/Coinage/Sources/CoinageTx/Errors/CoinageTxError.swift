import DurableTransactions
import Foundation

/// Failures raised by the durability subsystem.
public enum CoinageTxError: Error, Equatable {
    /// Registration rejected: the entry consumes nothing and mints nothing.
    case emptyEntry
    /// Registration rejected: an output is already an output, or a received-coin key, of
    /// another entry.
    case outputNotFresh(String)
    /// Registration rejected: an input is already claimed by an entry that is not a
    /// failure.
    case inputAlreadyClaimed(String)
    /// Registration rejected: an input carries a handoff mark.
    case inputHandedOff(String)
    /// Handoff rejected: an asset a live entry still claims cannot also leave the device.
    case handoffOfClaimedAsset(String)
    /// Handoff rejected: the asset already carries a handoff mark, so its keys have left — or are
    /// about to — for another peer.
    case handoffOfHandedOffAsset(String)
    /// Handoff rejected: no local row names this coin, so no mark can be written for it.
    case handoffOfUnknownAsset(String)
    /// The entry is not in the store.
    case entryNotFound(CoinageTxId)
    /// A pinned chain view could not be read, so the pass cannot run.
    case chainViewUnavailable
    /// The built extrinsic is immortal, so it carries no era window to recover it against.
    case notMortal
}

public extension CoinageTxError {
    /// The engine's rejections that coinage publishes under its own name; `nil` for the ones it does not
    /// rename, which callers see as the engine raised them.
    init?(durableTxError: DurableTxError) {
        switch durableTxError {
        case .notMortal: self = .notMortal
        case .chainViewUnavailable: self = .chainViewUnavailable
        case let .entryNotFound(id): self = .entryNotFound(id)
        case .unregisteredDomain,
             .buildIncomplete,
             .foreignRegistrationScope: return nil
        }
    }
}
