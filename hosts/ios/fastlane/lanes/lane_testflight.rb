desc "Submit a new build to Apple TestFlight"
desc "Example usage: fastlane upload_testflight changelog:'Release notes' distribute_external:true groups:['Group Name']"
lane :upload_testflight do |options|
  changelog = options[:changelog] || ''
  distribute_external = options[:distribute_external] || false
  # Wait for build processing if distributing to external group
  skip_waiting = !distribute_external

  # Only pass groups if distributing externally
  groups = distribute_external ? options[:groups] : nil

  upload_to_testflight(
    app_identifier: testflight_app_identifier,
    skip_waiting_for_build_processing: skip_waiting,
    changelog: changelog,
    distribute_external: distribute_external,
    groups: groups
  )
end

desc "Upload a build to TestFlight and return without waiting for Apple to process it"
desc "finish_testflight then distributes it, so a failure while Apple processes the"
desc "build is retried without uploading again."
lane :upload_testflight_binary do
  upload_to_testflight(
    app_identifier: testflight_app_identifier,
    skip_waiting_for_build_processing: true
  )
end

desc "Wait for an uploaded build to finish processing, then set its changelog and hand it to the external group"
desc "Reads BUILD_NUMBER, MARKETING_VERSION, RELEASE_NOTES and EXTERNAL_GROUP_NAME."
desc "Example usage: BUILD_NUMBER=42 MARKETING_VERSION=1.0 EXTERNAL_GROUP_NAME='Group' fastlane finish_testflight"
lane :finish_testflight do
  build_number = ENV["BUILD_NUMBER"].to_s
  app_version = ENV["MARKETING_VERSION"].to_s
  external_group = ENV["EXTERNAL_GROUP_NAME"].to_s
  UI.user_error!("BUILD_NUMBER env var is required") if build_number.empty?
  UI.user_error!("MARKETING_VERSION env var is required") if app_version.empty?
  UI.user_error!("EXTERNAL_GROUP_NAME env var is required") if external_group.empty?

  load_asc_api_key

  # App Store Connect answers the processing poll with an occasional 5xx, and
  # one bad answer ends the whole wait. The build is already uploaded, so each
  # attempt picks up where the last one stopped. Three attempts ride out a
  # passing error; more than that is an outage, and failing says so.
  attempts = 3
  attempts.times do |index|
    begin
      upload_to_testflight(
        app_identifier: testflight_app_identifier,
        distribute_only: true,
        app_version: app_version,
        build_number: build_number,
        changelog: ENV["RELEASE_NOTES"].to_s,
        distribute_external: true,
        groups: [external_group]
      )
      break
    rescue StandardError => error
      raise if index == attempts - 1
      UI.important("Distributing build #{build_number} failed on attempt #{index + 1} of #{attempts}: #{error.message}")
      sleep(60)
    end
  end
end

desc "Report whether a build number is already in App Store Connect, processed or not"
desc "Prints 'TestFlight build <n> uploaded: true|false' for the workflow to read."
desc "Example usage: BUILD_NUMBER=42 fastlane testflight_build_uploaded"
lane :testflight_build_uploaded do
  build_number = ENV["BUILD_NUMBER"].to_s
  UI.user_error!("BUILD_NUMBER env var is required") if build_number.empty?

  load_asc_api_key
  app = Spaceship::ConnectAPI::App.find(testflight_app_identifier)
  UI.user_error!("App not found for bundle id '#{testflight_app_identifier}'") if app.nil?

  # A build Apple has only just received is a delivery until it is listed as
  # a build, so both lists are read.
  built = Spaceship::ConnectAPI::Build.all(app_id: app.id, build_number: build_number).any?
  delivered = app.get_build_deliveries.any? { |delivery| delivery.cf_build_version.to_s == build_number }
  uploaded = built || delivered

  UI.message("TestFlight build #{build_number} uploaded: #{uploaded}")
  uploaded
end

# Without this the Appfile default wins and every post-upload step (changelog, external
# group) runs against the production App Store Connect record. Safetynet is a separate
# app with its own build-number sequence, so those steps silently find no build.
def testflight_app_identifier
  app_identifier = ENV["IOS_BUNDLE_ID"]
  app_identifier = CredentialsManager::AppfileConfig.try_fetch_value(:app_identifier) if app_identifier.to_s.empty?
  app_identifier
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
