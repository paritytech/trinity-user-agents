import Foundation
import ExtrinsicService
import SubstrateSdk
import KeyDerivation

/// Protocol for creating extrinsic origins.
///
/// The app-side implementation decides between People vs LitePeople origins
/// based on the person's membership status.
public protocol OriginCreating {
    /// Transaction must be signed by the coin's keypair.
    func createAsCoinOrigin(for wallet: WalletManaging) throws -> ExtrinsicOriginDefining

    /// A plain signed origin for `wallet` on `chainId` — the data store account paying its own fee.
    func createSignedOrigin(for wallet: WalletManaging, chainId: ChainId) async throws -> ExtrinsicOriginDefining

    /// Creates an InfallibleUnpaidSigned origin.
    ///
    /// Used for operations that require no fee payment and no ring proof —
    /// the extrinsic is submitted with the InfallibleUnpaidSigned mode.
    func createInfallibleUnpaidSignedOrigin(for wallet: WalletManaging) throws -> ExtrinsicOriginDefining

    /// Creates AsUnloadToken origins for multiple voucher groups in a single batch.
    ///
    /// Resolves all unload tokens, enabling concurrent extrinsic submission.
    /// Each group gets a distinct (period, counter) pair to avoid conflicts.
    ///
    /// - Parameters:
    ///   - voucherGroups: Array of voucher arrays, one per recycler group.
    ///   - currentDate: Current date for unload token period calculation.
    ///   - blockHash: Optional block hash for querying state at a specific block.
    /// - Returns: Array of origins, one per input group (same order).
    /// - Throws: CoinageCommonError.recyclerNotFound if any vouchers lack recycler info.
    func createAsUnloadTokenOrigins(
        voucherGroups: [[Voucher]],
        currentDate: Date,
        blockHash: BlockHashData?
    ) async throws -> [ExtrinsicOriginDefining]
}
