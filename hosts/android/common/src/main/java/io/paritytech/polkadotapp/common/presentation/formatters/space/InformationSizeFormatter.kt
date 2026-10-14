package io.paritytech.polkadotapp.common.presentation.formatters.space

import androidx.compose.runtime.staticCompositionLocalOf
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor
import javax.inject.Inject
import javax.inject.Singleton

val LocalInformationSizeFormatter = staticCompositionLocalOf<InformationSizeFormatter> {
    noLocalProvidedFor("InformationSizeFormatter")
}

interface InformationSizeFormatter {
    fun format(informationSize: InformationSize): String

    companion object {
        fun mocked(): InformationSizeFormatter = MockedInformationSizeFormatter()
    }
}

@Singleton
class RealInformationSizeFormatter @Inject constructor() : InformationSizeFormatter {
    override fun format(informationSize: InformationSize): String {
        return informationSize.toComponents { gigabytes, megabytes, kilobytes, bytes ->
            when {
                gigabytes > 0 -> gigabytes.toString() + "GB"
                megabytes > 0 -> megabytes.toString() + "MB"
                kilobytes > 0 -> kilobytes.toString() + "KB"
                else -> bytes.toString() + "B"
            }
        }
    }
}

private class MockedInformationSizeFormatter : InformationSizeFormatter {
    override fun format(informationSize: InformationSize): String {
        return "390 KB"
    }
}
