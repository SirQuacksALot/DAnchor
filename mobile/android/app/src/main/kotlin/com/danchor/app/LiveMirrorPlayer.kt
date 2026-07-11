package com.danchor.app

import android.media.MediaCodec
import android.media.MediaFormat
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import uniffi.danchor_ffi.FrameReassemblerFfi
import uniffi.danchor_ffi.NoiseHandshake
import uniffi.danchor_ffi.decodeEncrypted
import uniffi.danchor_ffi.decodeHandshake
import uniffi.danchor_ffi.decodeVideo
import uniffi.danchor_ffi.encodeHandshake
import uniffi.danchor_ffi.generateNoiseKeypair
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress

// Temporary Module 2b groundwork test: same secure-handshake dance as
// Networking.kt's establishSecureSession, but instead of one encrypted Ping
// it keeps the socket open and feeds every live-received Video fragment
// into the same MediaCodec-to-Surface pipeline VideoTestPlayer.kt proved
// standalone against local files. Remove once this becomes the real
// connection flow instead of a separate opt-in test path.
@Composable
fun LiveMirrorPlayer(
    host: String,
    port: Int,
    secretHex: String,
) {
    AndroidView(
        modifier = Modifier.fillMaxSize(),
        factory = { context ->
            SurfaceView(context).apply {
                holder.addCallback(
                    object : SurfaceHolder.Callback {
                        override fun surfaceCreated(holder: SurfaceHolder) {
                            Thread { receiveAndRenderLive(host, port, secretHex, holder.surface) }.start()
                        }

                        override fun surfaceChanged(
                            holder: SurfaceHolder,
                            format: Int,
                            width: Int,
                            height: Int,
                        ) {}

                        override fun surfaceDestroyed(holder: SurfaceHolder) {}
                    },
                )
            }
        },
    )
}

private fun receiveAndRenderLive(
    host: String,
    port: Int,
    secretHex: String,
    surface: Surface,
) {
    val psk = hexToBytes(secretHex) ?: return
    val socket = DatagramSocket()
    socket.soTimeout = 10_000
    val address = InetAddress.getByName(host)

    val keypair = generateNoiseKeypair()
    val handshake = NoiseHandshake.initiator(keypair.private, psk)

    fun sendHandshake(message: ByteArray) {
        val bytes = encodeHandshake(0u, message)
        socket.send(DatagramPacket(bytes, bytes.size, address, port))
    }

    fun receiveHandshake(): ByteArray? {
        val buf = ByteArray(2048)
        val reply = DatagramPacket(buf, buf.size)
        socket.receive(reply)
        return decodeHandshake(reply.data.copyOf(reply.length))
    }

    val message1 = handshake.writeNext() ?: return
    sendHandshake(message1)
    val message2 = receiveHandshake() ?: return
    if (!handshake.readNext(message2)) return
    val message3 = handshake.writeNext() ?: return
    sendHandshake(message3)
    val channel = handshake.intoSession() ?: return

    val format = MediaFormat.createVideoFormat("video/avc", 1920, 1080)
    val codec = MediaCodec.createDecoderByType("video/avc")
    codec.configure(format, surface, null, 0)
    codec.start()

    val reassembler = FrameReassemblerFfi(16u)
    val buf = ByteArray(65536)

    while (true) {
        val packet = DatagramPacket(buf, buf.size)
        try {
            socket.receive(packet)
        } catch (e: Exception) {
            break
        }

        val encrypted = decodeEncrypted(packet.data.copyOf(packet.length)) ?: continue
        val plaintext = channel.decrypt(encrypted.sequence, encrypted.ciphertext) ?: continue
        val fragment = decodeVideo(plaintext) ?: continue
        val complete = reassembler.insert(fragment) ?: continue

        val inputIndex = codec.dequeueInputBuffer(10_000)
        if (inputIndex >= 0) {
            val inputBuffer = codec.getInputBuffer(inputIndex)
            if (inputBuffer != null) {
                inputBuffer.clear()
                inputBuffer.put(complete.data)
                codec.queueInputBuffer(inputIndex, 0, complete.data.size, System.nanoTime() / 1000, 0)
            }
        }

        val bufferInfo = MediaCodec.BufferInfo()
        var outputIndex = codec.dequeueOutputBuffer(bufferInfo, 10_000)
        while (outputIndex >= 0) {
            codec.releaseOutputBuffer(outputIndex, true)
            outputIndex = codec.dequeueOutputBuffer(bufferInfo, 0)
        }
    }

    codec.stop()
    codec.release()
}
