private_lane :setup_ci_keychain do
  return unless is_ci

  create_keychain(
    name: "github_actions_keychain",
    password: ENV["KEYCHAIN_PASSWORD"],
    default_keychain: true,
    unlock: true,
    timeout: 3600,
    add_to_search_list: true,
    lock_when_sleeps: false
  )
end

desc "Prepares certificate and provisioning profile"
lane :prepare_code_signing do
  begin
    app_identifier = ENV["IOS_BUNDLE_ID"] || raise("Missing IOS_BUNDLE_ID environment variable")
    extension_identifier = ENV["IOS_EXTENSION_BUNDLE_ID"]
    export_method = ENV["EXPORT_METHOD"] || raise("Missing EXPORT_METHOD environment variable")

    setup_ci_keychain

    # Prepare array of identifiers to fetch profiles for
    identifiers = [app_identifier]
    identifiers << extension_identifier if extension_identifier && !extension_identifier.empty?

    match_type_mapping = {
      "ad-hoc" => "adhoc",
      "app-store" => "appstore",
      "development" => "development"
    }

    match_type = match_type_mapping[export_method] || raise("Invalid EXPORT_METHOD: #{export_method}")

    match_config = {
      app_identifier: identifiers,
      readonly: true,
      keychain_name: is_ci ? "github_actions_keychain" : nil,
      keychain_password: is_ci ? ENV["KEYCHAIN_PASSWORD"] : nil,
      type: match_type
    }

    match(match_config)
  rescue => ex
    UI.error("Failed to prepare code signing: #{ex.message}")
    raise
  end
end

desc "Updates signing data using App Store Connect API"
desc "Updates the Development/AdHoc profiles for DevCI and AppStore profiles for TestFlight"
lane :update_signing_data do
  begin
    setup_ci_keychain

    api_key = app_store_connect_api_key(
      key_id: ENV["ASC_KEY_ID"],
      issuer_id: ENV["ASC_ISSUER_ID"],
      key_content: ENV["ASC_KEY_BASE64"],
      is_key_content_base64: true,
      duration: 1200,
      in_house: false
    )

    signing_profiles = {
      "development" => [
        "io.parity.polkadotapp.develop",
        "io.parity.polkadotapp.develop.NotificationServiceExtension"
      ],
      "adhoc" => [
        "io.parity.polkadotapp.develop",
        "io.parity.polkadotapp.develop.NotificationServiceExtension"
      ],
      "appstore" => [
        "io.parity.polkadotapp",
        "io.parity.polkadotapp.NotificationServiceExtension",
        "io.parity.polkadotapp.safety",
        "io.parity.polkadotapp.safety.NotificationServiceExtension"
      ]
    }

    signing_profiles.each do |type, identifiers|
      match(
        type: type,
        app_identifier: identifiers,
        api_key: api_key,
        readonly: false,
        force_for_new_devices: type != "appstore",
        keychain_name: is_ci ? "github_actions_keychain" : nil,
        keychain_password: is_ci ? ENV["KEYCHAIN_PASSWORD"] : nil
      )
    end
  rescue => ex
    UI.error("Failed to update signing data: #{ex.message}")
    raise
  end
end
