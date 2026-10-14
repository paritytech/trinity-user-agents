import Testing
@testable import Coinage

// NOTE: The previous 994-line CoinageServiceTests suite targeted an API surface that has since been
// replaced wholesale and was already `.disabled()`:
//   - the balance streams it asserted on (`spendableBalanceStream`, `$0.fullPrivacy`) no longer exist
//     — the balance service now exposes `totalBalanceStream` / `lockedBalanceStream`;
//   - `CoinageService` now takes `trackedCoinProvider` / `trackedVoucherProvider` (of `TrackedCoin` /
//     `TrackedVoucher`), not raw coin/voucher providers;
//   - the coin `markRecycling` / `markAvailable` / `markSpent` bookkeeping it drove was removed in the
//     move to derive-on-read state.
//
// Rather than make obsolete, non-running code compile, it was reset to this placeholder. Its coverage
// — balance calculation and transfer previews — is being re-established by the new ports.
@Suite("CoinageService Tests", .disabled())
struct CoinageServiceTests {}
