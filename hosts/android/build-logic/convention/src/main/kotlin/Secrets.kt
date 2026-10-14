import com.android.build.api.dsl.VariantDimension
import java.util.Properties

fun VariantDimension.buildConfigString(name: String, value: String) {
    val escapedValue = value
        .replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("\n", "\\n")
        .replace("\r", "\\r")

    buildConfigField("String", name, "\"$escapedValue\"")
}

/**
 * Only for values that must never be defaulted silently: a missing secret fails the build
 * instead of shipping a placeholder. Prefer this over [readSecretOrDefault] everywhere
 * outside `signingConfigs`.
 */
fun Properties.readSecretOrThrow(secretName: String): String {
    return readSecretOrNull(secretName)
        ?: error("Missing secret '$secretName'. Add it to local.properties or provide it as an environment variable.")
}

fun Properties.readSecretOrDefault(secretName: String, default: String): String {
    return readSecretOrNull(secretName) ?: default
}

fun Properties.readSecretOrNull(secretName: String): String? {
    val secret = getProperty(secretName) ?: System.getenv(secretName)
    return secret?.takeIf { it.isNotEmpty() }
}
