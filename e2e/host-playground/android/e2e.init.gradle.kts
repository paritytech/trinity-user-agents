// Adds the host-playground e2e hooks to the Android host's nightly build.
//
//   hosts/android/gradlew --init-script e2e/host-playground/android/e2e.init.gradle.kts :app:assembleGpNightly
//
// The hooks live outside hosts/android, which is vendored and refreshed from
// upstream, so they are included as an extra project and attached to `:app`
// only through the nightly configuration. Gradle runs init scripts for every
// build, so the included build-logic build is left alone.

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
