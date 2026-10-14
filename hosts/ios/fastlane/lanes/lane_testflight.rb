desc "Submit a new build to Apple TestFlight"
desc "Example usage: fastlane upload_testflight changelog:'Release notes' distribute_external:true groups:['Group Name']"
lane :upload_testflight do |options|
  changelog = options[:changelog] || ''
  distribute_external = options[:distribute_external] || false
  # Wait for build processing if distributing to external group
  skip_waiting = !distribute_external

  # Only pass groups if distributing externally
  groups = distribute_external ? options[:groups] : nil

  # Without this the Appfile default wins and every post-upload step (changelog, external
  # group) runs against the production App Store Connect record. Safetynet is a separate
  # app with its own build-number sequence, so those steps silently find no build.
  app_identifier = ENV["IOS_BUNDLE_ID"]
  app_identifier = CredentialsManager::AppfileConfig.try_fetch_value(:app_identifier) if app_identifier.to_s.empty?

  upload_to_testflight(
    app_identifier: app_identifier,
    skip_waiting_for_build_processing: skip_waiting,
    changelog: changelog,
    distribute_external: distribute_external,
    groups: groups
  )
end

desc "Load ASC API Key information to use in subsequent lanes"
lane :load_asc_api_key do
  app_store_connect_api_key(
    key_id: ENV['ASC_KEY_ID'],
    issuer_id: ENV['ASC_ISSUER_ID'],
    key_content: ENV['ASC_KEY_BASE64'],
    is_key_content_base64: true,
    in_house: false
  )
end

desc "Get the highest build number ever uploaded to TestFlight"
desc "Example usage: fastlane get_testflight_build_number"
lane :get_testflight_build_number do
  load_asc_api_key

  # Use the highest build number across all builds instead of `latest_testflight_build_number`:
  # that action returns the most recently uploaded build, so a manual upload with a lower number
  # drags the counter backwards. Taking the max keeps it monotonic.
  # Nightly and Release are separate App Store Connect records with independent
  # build-number sequences, so the caller selects which one to query.
  app_identifier = ENV["IOS_BUNDLE_ID"]
  app_identifier = CredentialsManager::AppfileConfig.try_fetch_value(:app_identifier) if app_identifier.to_s.empty?
  app = Spaceship::ConnectAPI::App.find(app_identifier)
  UI.user_error!("App not found for bundle id '#{app_identifier}'") if app.nil?

  builds = Spaceship::ConnectAPI::Build.all(app_id: app.id)
  build_number = builds.map { |build| build.version.to_i }.max || 0

  UI.message("Latest TestFlight build number: #{build_number}")
  build_number
end
