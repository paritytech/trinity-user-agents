import Foundation

public enum CoinageConstants {
    /// Upper bound of the recycler-fungibility scale (percentage).
    public static let fullFungibility: UInt8 = 100

    /// How long a sent chat payment's transfer keeps being rebuilt before giving up (6 hours).
    public static let chatPaymentRetryWindow: TimeInterval = 6 * 60 * 60

    /// How long after a payment message was sent its coins are still worth trying to claim (7 days).
    public static let claimRetryWindow: TimeInterval = 7 * 24 * 60 * 60

    /// How long an incoming top-up keeps trying before whatever it has is all it will ever have (1 hour).
    public static let topUpRetryWindow: TimeInterval = 60 * 60

    /// How long an external payment's transactions keep being rebuilt before it is given up (5 minutes).
    public static let externalPaymentRetryWindow: TimeInterval = 5 * 60
}
