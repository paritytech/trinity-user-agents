import Foundation

// Built from individual Firebase RemoteConfig keys:
//   identity_backend_url, ipfs_gateway_url, game_dashboard_url, dot_ns_config, coinage_instance_id,
//   account_data_store_config { contractAddress }, payment_asset_config {
//   symbol, iconSquareUrl, iconWideUrl }, app_sharing_url
// Each field nil if the corresponding key is missing or empty.
struct RemoteAppConfig {
    let identityBackendUrl: URL?
    let ipfsGatewayUrl: URL?
    let gameDashboardUrl: URL?
    let dotNsResolver: String?
    /// Absent in payloads published before manifest support, which disables manifest
    /// resolution and leaves legacy resolution working.
    let dotNsNameRegistry: String?
    let coinageInstanceId: UInt32?
    /// The `AccountDataStore` contract on Asset Hub from the `account_data_store_config` remote object,
    /// already checked to be an EVM address by `FirebaseApplicationService`
    let accountDataStoreContract: Data?
    /// The payment asset's symbol and logo URLs from the `payment_asset_config` remote object.
    let paymentAsset: PaymentAssetConfig?
    let appSharingUrl: URL?
}

extension RemoteAppConfig {
    var isValid: Bool {
        var result = identityBackendUrl != nil
            && ipfsGatewayUrl != nil
            && dotNsResolver != nil
            && coinageInstanceId != nil

        #if TESTNET_FEATURE
            result = result && gameDashboardUrl != nil
        #endif

        return result
    }
}
