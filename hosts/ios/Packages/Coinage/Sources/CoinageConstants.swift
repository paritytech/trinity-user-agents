import Foundation

public enum CoinageConstants {
    /// Upper bound of the recycler-fungibility scale (percentage).
    public static let fullFungibility: UInt8 = 100

    /// Interval at which the background recycling task is scheduled (24 hours).
    public static let backgroundRecyclingInterval: TimeInterval = 24 * 60 * 60

    /// How long a received transfer's coins are still worth trying to claim (6 hours). Bounds the
    /// claim retry loop; measured from when the message is first seen.
    public static let claimRetryWindow: TimeInterval = 6 * 60 * 60

    /// How long a claim from raw secret keys (top-up / recovery) keeps retrying before giving up.
    /// Shorter than ``claimRetryWindow`` — these callers await the outcome inline.
    public static let secretKeyClaimTimeout: TimeInterval = 60

    /// How long an incoming top-up keeps trying before whatever it has is all it will ever have (1
    /// hour). Measured from when the operation opened, not from an individual attempt, so a resumed
    /// top-up finishes the window it was given.
    public static let topUpRetryWindow: TimeInterval = 60 * 60

    /// Coin age threshold at which coin is still operatable
    public static let coinMaxAge: Int16 = 16

    /// Coin age threshold at which recycling is triggered (coinMaxAge - 2).
    public static let recycleAtAge: Int16 = coinMaxAge - 2

    /// Lookback window (in seconds) for unload token period calculation (1 hour).
    static let periodLookbackInterval: UInt64 = 3_600

    /// Maximum random wait time before a voucher becomes ready (6 hours).
    static let maxVoucherWaitTime: TimeInterval = 6 * 60 * 60

    /// Key-derivation path components for coinage keys. Coins derive under
    /// `//coinage//<purse>//<page>/<item>` and vouchers under
    /// `//coinage-ring-vrf//<purse>//<page>//<item>`, where `<page>` is the installation id
    /// (see ``CoinageInstallationId/pageSegment``).
    public enum Derivation {
        /// `MAIN_PURSE` — the single purse all coinage keys derive under.
        static let mainPurse: UInt32 = 4_294_967_295
    }
}
