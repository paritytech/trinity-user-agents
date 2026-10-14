package io.paritytech.polkadotapp.tools_media_connection_impl

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import org.webrtc.DefaultVideoDecoderFactory
import org.webrtc.DefaultVideoEncoderFactory
import org.webrtc.EglBase
import org.webrtc.PeerConnectionFactory
import org.webrtc.audio.JavaAudioDeviceModule
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class WebRtcCore @Inject constructor(
    @param:ApplicationContext private val context: Context
) {
    val eglBase: EglBase by lazy { EglBase.create() }

    val peerConnectionFactory: PeerConnectionFactory by lazy { createPeerConnectionFactory() }

    private fun createPeerConnectionFactory(): PeerConnectionFactory {
        val options = PeerConnectionFactory
            .InitializationOptions
            .builder(context)
            .createInitializationOptions()

        PeerConnectionFactory.initialize(options)

        val audioDeviceModule = JavaAudioDeviceModule.builder(context)
            .setUseHardwareAcousticEchoCanceler(true)
            .setUseHardwareNoiseSuppressor(true)
            .setUseLowLatency(true)
            .createAudioDeviceModule()

        val encoderFactory = DefaultVideoEncoderFactory(eglBase.eglBaseContext, true, true)
        val decoderFactory = DefaultVideoDecoderFactory(eglBase.eglBaseContext)

        return PeerConnectionFactory.builder()
            .setAudioDeviceModule(audioDeviceModule)
            .setVideoEncoderFactory(encoderFactory)
            .setVideoDecoderFactory(decoderFactory)
            .createPeerConnectionFactory()
    }
}
