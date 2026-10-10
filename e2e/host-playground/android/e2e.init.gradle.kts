// Adds the e2e hooks to the nightly build without touching hosts/android.
//
//   hosts/android/gradlew --init-script e2e/host-playground/android/e2e.init.gradle.kts :app:assembleGpNightly
//
// Init scripts run for every build, so the included build-logic build is skipped.

val hooksDir = initscript.sourceFile!!.parentFile.resolve("hooks")

settingsEvaluated {
    if (findProject(":app") == null) return@settingsEvaluated
    include(":e2e-hooks")
    project(":e2e-hooks").projectDir = hooksDir
}

beforeProject {
    if (path == ":app") {
        configurations.matching { it.name == "nightlyImplementation" }.configureEach {
            dependencies.add(project.dependencies.project(mapOf("path" to ":e2e-hooks")))
        }
    }
}
