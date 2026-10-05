import Foundation
import Operation_iOS
import FirebaseCore
import FirebaseRemoteConfig
import Combine
import ChainRegistry
import Revive

protocol RemoteConfigDelegate: AnyObject {
    func remoteConfig(didFinishLoading result: Result<Void, Error>)
    func remoteConfig(appVersionDidChange result: Result<String, Error>)
}

extension RemoteConfigDelegate {
    func remoteConfig(didFinishLoading _: Result<Void, Error>) {}
    func remoteConfig(appVersionDidChange _: Result<String, Error>) {}
}

final class FirebaseApplicationService: RemoteConfigManaging {
    static let shared = FirebaseApplicationService()

    // MARK: Properties

    private let remoteConfig: FirebaseRemoteConfig.RemoteConfig

    weak var delegate: (any RemoteConfigDelegate)?
    private let logger: LoggerProtocol = Logger.shared

    // MARK: Initial methods

    private init() {
        #if DEBUG
            var args = ProcessInfo.processInfo.arguments
            args.append("-FIRDebugEnabled")
            ProcessInfo.processInfo.setValue(args, forKey: "arguments")
            FirebaseConfiguration.shared.setLoggerLevel(.info)
        #else
            var args = ProcessInfo.processInfo.arguments
            args.append("-FIRDebugDisabled")
            ProcessInfo.processInfo.setValue(args, forKey: "arguments")
        #endif

        FirebaseApp.configure()
        remoteConfig = FirebaseRemoteConfig.RemoteConfig.remoteConfig()

        configurationRemoteConfigSettings()
    }

    // MARK: Public methods

    func fetchRemoteConfigValues() {
        Task {
            do {
                try await fetchAndActivateRemoteConfig()
                delegate?.remoteConfig(didFinishLoading: .success(()))
            } catch {
                delegate?.remoteConfig(didFinishLoading: .failure(error))
            }
        }
    }

    func fetchAndActivateRemoteConfig() async throws {
        let signal: CustomSignal = .environment
        try await remoteConfig.setCustomSignals([signal.key: signal.value])
        let status = try await remoteConfig.fetchAndActivate()
        handleRemoteConfigStatus(status)
    }

    func asyncWaitChainsForRemoteConfigValues() -> CompoundOperationWrapper<[RemoteChainModel]> {
        asyncWaitForRemoteConfigValues(for: .chains)
    }

    func asyncWaitXcmTransfers<T: Decodable>() -> CompoundOperationWrapper<T> {
        asyncWaitForRemoteConfigValues(for: .xcmTransfers)
    }

    func asyncWaitXcmGeneralConfig<T: Decodable>() -> CompoundOperationWrapper<T> {
        asyncWaitForRemoteConfigValues(for: .generalXcmConfig)
    }

    func asyncWaitGameResultsFallbackURL() -> CompoundOperationWrapper<URL> {
        asyncWaitForRemoteConfigValues(for: .gameResultsFallbackURL)
    }

    func asyncWaitW3sMerchants<T: Decodable>() -> CompoundOperationWrapper<T> {
        asyncWaitForRemoteConfigValues(for: .w3sMerchants)
    }

    func asyncWaitCollectiblesFallbackURL() -> CompoundOperationWrapper<URL> {
        asyncWaitForRemoteConfigValues(for: .collectiblesFallbackURL)
    }

    func syncedCollectiblesEnabled() -> Bool {
        remoteConfig[.collectiblesEnabled].boolValue
    }

    func syncedTxExtensionVersions() -> [ChainModel.Id: UInt8] {
        guard let json = remoteConfig[.txExtensionVersions].jsonValue as? [String: Any] else {
            return [:]
        }

        return json.reduce(into: [:]) { result, entry in
            guard let number = entry.value as? NSNumber else { return }
            result[entry.key] = number.uint8Value
        }
    }

    func syncedAppConfig() -> RemoteAppConfig {
        RemoteAppConfig(
            identityBackendUrl: url(for: .identityBackendUrl),
            ipfsGatewayUrl: url(for: .ipfsGatewayUrl),
            gameDashboardUrl: url(for: .gameDashboardUrl),
            dotNsResolver: dotNsResolverAddress(),
            dotNsNameRegistry: dotNsNameRegistryAddress(),
            coinageInstanceId: coinageInstanceId(),
            fundingUrl: fundingConfigValue(.onrampUrl),
            offrampUrl: fundingConfigValue(.offrampUrl),
            accountDataStoreContract: accountDataStoreContractAddress(),
            paymentAsset: paymentAssetConfig(),
            appSharingUrl: url(for: .appSharingUrl)
        )
    }

    func asyncWaitRemoteConfig() async throws -> RemoteAppConfig {
        syncedAppConfig()
    }

    func syncedIssueProxyConfiguration() throws -> IssueProxyConfiguration {
        try IssueProxyConfiguration(
            endpoint: remoteConfig[.issueProxyUrl].stringValue,
            apiKey: remoteConfig[.issueProxyApiKey].stringValue
        )
    }
}

private extension FirebaseApplicationService {
    // MARK: Private methods

    private func configurationRemoteConfigSettings() {
        let remoteConfigSettings = RemoteConfigSettings()
        remoteConfigSettings.minimumFetchInterval = .zero
        remoteConfigSettings.fetchTimeout = 10
        remoteConfig.configSettings = remoteConfigSettings
    }

    private func handleRemoteConfigStatus(_ status: RemoteConfigFetchAndActivateStatus) {
        switch status {
        case .successFetchedFromRemote:
            logger.info("RemoteConfig fetched from remote and activated")
        case .successUsingPreFetchedData:
            logger.info("RemoteConfig activated using pre-fetched data")
        case .error:
            logger.error("Error during RemoteConfig activation")
        @unknown default:
            logger.error("Unknown status during RemoteConfig activation")
        }

        let appVersion = remoteConfig[.latestAppVersion].stringValue
        guard !appVersion.isEmpty else {
            logger.error("App version not found in RemoteConfig")
            delegate?.remoteConfig(appVersionDidChange: .failure(RemoteConfigError.versionNotFound))
            return
        }
        logger.info("Fetched latest app version: \(appVersion)")
        delegate?.remoteConfig(appVersionDidChange: .success(appVersion))
    }

    func nonEmptyString(for key: String) -> String? {
        let value = remoteConfig[key].stringValue
        return value.isEmpty ? nil : value
    }

    func url(for key: String) -> URL? {
        guard let value = nonEmptyString(for: key) else { return nil }
        return URL(string: value)
    }

    /// One JSON object shared with Android, passed through as published:
    /// `{ "onrampUrl": "getcash.dot", "offrampUrl": "https://getcash.dot/offramp" }`.
    func fundingConfigValue(_ field: String) -> String? {
        let json = remoteConfig[.fundingConfig].jsonValue as? [String: String]
        guard let value = json?[field], !value.isEmpty else { return nil }
        return value
    }

    /// One JSON object shared with Android: `{ "contractAddress": "0x…" }`, decoded to an EVM address
    /// here so consumers never see a malformed one. A delivered address that cannot be used is a
    /// config mistake, not a payload still on its way: both stall registration, only the log tells them apart.
    func accountDataStoreContractAddress() -> EvmAddress? {
        let json = remoteConfig[.accountDataStoreConfig].jsonValue as? [String: String]
        guard let hex = json?[.contractAddress], !hex.isEmpty else { return nil }

        guard let address = try? EvmAddressFormat.validate(Data(hexString: hex)) else {
            logger.error("Remote config carries an unusable AccountDataStore contract address: \(hex)")
            return nil
        }
        return address
    }

    func paymentAssetConfig() -> PaymentAssetConfig? {
        guard let json = remoteConfig[.paymentAssetConfig].jsonValue as? [String: String] else { return nil }
        return PaymentAssetConfig(json: json)
    }

    func dotNsConfigEntry(_ field: String, treatingEmptyAsMissing: Bool = false) -> String? {
        let json = remoteConfig[.dotNsResolver].jsonValue as? [String: String]
        guard let value = json?[field] else { return nil }

        return treatingEmptyAsMissing && value.isEmpty ? nil : value
    }

    func dotNsResolverAddress() -> String? {
        dotNsConfigEntry("resolverContractAddress")
    }

    func dotNsNameRegistryAddress() -> String? {
        // Empty counts as absent: payloads published before manifest support carry no name
        // registry, and an empty address would read as a configured one.
        dotNsConfigEntry("registryContractAddress", treatingEmptyAsMissing: true)
    }

    func coinageInstanceId() -> UInt32? {
        guard let value = nonEmptyString(for: .coinageInstanceId) else { return nil }
        return UInt32(value)
    }

    func asyncWaitForRemoteConfigValues<T: Decodable>(for key: String) -> CompoundOperationWrapper<T> {
        CompoundOperationWrapper(targetOperation: AsyncClosureOperation<T>(
            operationClosure: { [logger, weak self] closure in
                guard let self else {
                    return
                }

                let data = remoteConfig[key].dataValue
                do {
                    let models = try JSONDecoder().decode(T.self, from: data)

                    closure(.success(models))
                } catch {
                    logger.error("Failed to decode remote config: \(error) \(key)")
                    closure(.failure(error))
                }
            },
            cancelationClosure: {}
        ))
    }
}

private extension FirebaseApplicationService {
    enum CustomSignal {
        case environment

        var key: String {
            switch self {
            case .environment:
                "environment"
            }
        }

        var value: FirebaseRemoteConfig.CustomSignalValue {
            switch self {
            case .environment:
                #if UNSTABLE
                    "unstable"
                #elseif NIGHTLY
                    "nightly"
                #else
                    "release"
                #endif
            }
        }
    }
}

// MARK: - Constants

private extension String {
    static let latestAppVersion = "latest_ios_version"
    static let chains = "chains_v2"
    static let xcmTransfers = "cross_chain_transfers"
    static let generalXcmConfig = "xcm_general_config"
    static let gameResultsFallbackURL = "game_results_fallback_url"
    static let w3sMerchants = "w3s_merchants"
    static let collectiblesFallbackURL = "collectibles_fallback_url"
    static let collectiblesEnabled = "collectibles_enabled"
    static let txExtensionVersions = "transaction_extension_versions"
    static let identityBackendUrl = "identity_backend_url"
    static let ipfsGatewayUrl = "ipfs_gateway_url"
    static let gameDashboardUrl = "game_dashboard_url"
    static let dotNsResolver = "dot_ns_config"
    static let coinageInstanceId = "coinage_instance_id"
    static let fundingConfig = "funding_config"
    static let onrampUrl = "onrampUrl"
    static let offrampUrl = "offrampUrl"
    static let accountDataStoreConfig = "account_data_store_config"
    static let contractAddress = "contractAddress"
    static let paymentAssetConfig = "payment_asset_config"
    static let appSharingUrl = "app_sharing_url"
    static let issueProxyUrl = "issue_proxy_url"
    static let issueProxyApiKey = "issue_proxy_api_key"
}
