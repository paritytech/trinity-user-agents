#!/usr/bin/env kotlin

import java.io.File
import kotlin.system.exitProcess

println("--- New Module Creation Script ---")

val rawMainModule = prompt("Enter the main module name (e.g., tools, feature):")
val rawSubModule = prompt("Enter the submodule name (e.g., push-notifications, wallet):")
val useApiImpl = prompt("Enable api/impl separation? (y/n):").lowercase()

val projectRoot = File(".").absoluteFile
val settingsFile = File(projectRoot, "settings.gradle.kts")

if (settingsFile.exists().not()) {
    println("Error: settings.gradle.kts not found at ${settingsFile.path}")
    println("Please run this script from the project root directory (e.g., ./developer-tools/create_module.kts)")
    exitProcess(1)
}

if (useApiImpl == "y") {
    println("Creating modules with api/impl...")
    val apiTargetDir = File(projectRoot, "$rawMainModule/$rawSubModule/api")
    val implTargetDir = File(projectRoot, "$rawMainModule/$rawSubModule/impl")

    createModule(
        mainModule = rawMainModule,
        subModule = rawSubModule,
        suffix = "api",
        targetDir = apiTargetDir
    )

    createModule(
        mainModule = rawMainModule,
        subModule = rawSubModule,
        suffix = "impl",
        targetDir = implTargetDir
    )

    updateSettings(settingsFile, rawMainModule, rawSubModule, "api")
    updateSettings(settingsFile, rawMainModule, rawSubModule, "impl")
} else {
    println("Creating simple module...")
    val targetDir = File(projectRoot, "$rawMainModule/$rawSubModule")

    createModule(
        mainModule = rawMainModule,
        subModule = rawSubModule,
        suffix = null,
        targetDir = targetDir
    )

    updateSettings(settingsFile, rawMainModule, rawSubModule, null)
}

println("--- Module creation and settings update finished! ---")
println("Don't forget to sync your project in the IDE.")

fun prompt(text: String): String {
    println(text)
    return readlnOrNull() ?: run {
        println("Error: Failed to read input.")
        exitProcess(1)
    }
}

fun createModule(
    mainModule: String,
    subModule: String,
    suffix: String?,
    targetDir: File
) {
    if (targetDir.exists()) {
        println("Error: Module at path ${targetDir.path} already exists. Skipping...")
        return
    }

    val cleanMainModule = mainModule.replace("-", "_")
    val cleanSubModule = subModule.replace("-", "_")
    val featureName = if (suffix != null) {
        "${cleanMainModule}_${cleanSubModule}_$suffix"
    } else {
        "${cleanMainModule}_${cleanSubModule}"
    }

    println("Creating module: $featureName in ${targetDir.path}")

    val packageDir = File(targetDir, "src/main/kotlin/io/paritytech/polkadotapp/$featureName")
    try {
        if (!packageDir.mkdirs()) {
            println("Error: Could not create directory structure at ${packageDir.path}")
            return
        }
    } catch (e: Exception) {
        println("Error creating directories: ${e.message}")
        return
    }

    val buildFile = File(targetDir, "build.gradle.kts")

    try {
        buildFile.writeText(buildFileContent(featureName))
    } catch (e: Exception) {
        println("Error writing ${buildFile.path}: ${e.message}")
        return
    }

    println("Successfully created module: $featureName")
}

fun updateSettings(settingsFile: File, mainModule: String, subModule: String, suffix: String?) {
    val includePath = if (suffix != null) ":$mainModule:$subModule:$suffix" else ":$mainModule:$subModule"
    val newIncludeLine = "include(\"$includePath\")"

    if (!settingsFile.exists()) {
        println("Warning: settings.gradle.kts not found at ${settingsFile.path}. Cannot add module.")
        return
    }
    val lines = settingsFile.readLines().toMutableList()

    if (lines.any { it.trim() == newIncludeLine }) {
        println("Module $includePath already in settings.gradle.kts. Skipping.")
        return
    }

    val blockPrefix = "include(\":$mainModule:"
    val blockStartIndex = lines.indexOfFirst { it.trim().startsWith(blockPrefix) }

    if (blockStartIndex == -1) {
        val lastIncludeIndex = lines.indexOfLast { it.trim().startsWith("include(") }
        if (lastIncludeIndex != -1) {
            lines.add(lastIncludeIndex + 1, "")
            lines.add(lastIncludeIndex + 2, newIncludeLine)
        } else {
            lines.add(newIncludeLine)
        }
        println("Warning: Group for '$mainModule' not found in settings.gradle.kts. Appended to end of includes.")
    } else {
        val blockEndIndex = lines.indexOfLast { it.trim().startsWith(blockPrefix) }
        var inserted = false
        for (i in blockStartIndex..blockEndIndex) {
            val line = lines[i].trim()
            if (line.isEmpty() || !line.startsWith("include(")) continue // Skip empty lines or comments

            if (newIncludeLine < line) {
                lines.add(i, newIncludeLine)
                inserted = true
                break
            }
        }
        if (!inserted) {
            lines.add(blockEndIndex + 1, newIncludeLine)
        }
    }

    try {
        settingsFile.writeText(lines.joinToString("\n"))
        println("Successfully added $includePath to settings.gradle.kts")
    } catch (e: Exception) {
        println("Error writing to settings.gradle.kts: ${e.message}")
    }
}

fun buildFileContent(featureName: String): String {
    return """
    plugins {
    }

    android {
        namespace = "io.paritytech.polkadotapp.$featureName"
    }

    dependencies {

    }
    """.trimIndent()
}
