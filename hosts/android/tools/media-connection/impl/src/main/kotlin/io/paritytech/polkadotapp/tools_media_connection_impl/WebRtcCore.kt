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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

internal enum class AudioCaptureState { IDLE, LIVE, FAILED }

@Singleton
internal class WebRtcCore @Inject constructor(
    @param:ApplicationContext private val context: Context
) {
    val eglBase: EglBase by lazy { EglBase.create() }

    val peerConnectionFactory: PeerConnectionFactory by lazy { createPeerConnectionFactory() }
    val audioCaptureState = MutableStateFlow(AudioCaptureState.IDLE)

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
            .setAudioRecordStateCallback(object : JavaAudioDeviceModule.AudioRecordStateCallback {
                override fun onWebRtcAudioRecordStart() { audioCaptureState.value = AudioCaptureState.LIVE }
                override fun onWebRtcAudioRecordStop() { audioCaptureState.update { if (it == AudioCaptureState.FAILED) it else AudioCaptureState.IDLE } }
            })
            .setAudioRecordErrorCallback(object : JavaAudioDeviceModule.AudioRecordErrorCallback {
                override fun onWebRtcAudioRecordInitError(message: String) { audioCaptureState.value = AudioCaptureState.FAILED }
                override fun onWebRtcAudioRecordStartError(code: JavaAudioDeviceModule.AudioRecordStartErrorCode, message: String) { audioCaptureState.value = AudioCaptureState.FAILED }
                override fun onWebRtcAudioRecordError(message: String) { audioCaptureState.value = AudioCaptureState.FAILED }
            })
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
