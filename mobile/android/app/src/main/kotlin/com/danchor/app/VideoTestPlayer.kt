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
import java.io.File

// Temporary Module 2b groundwork test: decodes a directory of individually
// dumped H.264 access units (one file per frame - see danchor-desktop's
// `--capture-test`, which writes exactly this shape to
// /tmp/danchor-capture-frames/) via MediaCodec, rendering directly onto a
// SurfaceView. Proves hardware decode + render works on this tablet before
// any network wiring exists. Remove once Module 2b's real streaming path
// (decoding frames arriving live over the wire, not from local files) lands.
@Composable
fun VideoTestPlayer(framesDir: File) {
    AndroidView(
        modifier = Modifier.fillMaxSize(),
        factory = { context ->
            SurfaceView(context).apply {
                holder.addCallback(
                    object : SurfaceHolder.Callback {
                        override fun surfaceCreated(holder: SurfaceHolder) {
                            Thread { decodeAndRender(framesDir, holder.surface) }.start()
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

private fun decodeAndRender(
    framesDir: File,
    surface: Surface,
) {
    val files = framesDir.listFiles()?.sortedBy { it.name } ?: return
    if (files.isEmpty()) return

    val format = MediaFormat.createVideoFormat("video/avc", 1920, 1080)
    val codec = MediaCodec.createDecoderByType("video/avc")
    codec.configure(format, surface, null, 0)
    codec.start()

    var presentationTimeUs = 0L
    // Matches danchor-desktop's observed capture rate (~11fps) closely
    // enough for a visual proof - not meant to be frame-accurate.
    val frameIntervalUs = 90_000L

    for (file in files) {
        val bytes = file.readBytes()
        val inputIndex = codec.dequeueInputBuffer(10_000)
        if (inputIndex >= 0) {
            val inputBuffer = codec.getInputBuffer(inputIndex)
            if (inputBuffer != null) {
                inputBuffer.clear()
                inputBuffer.put(bytes)
                codec.queueInputBuffer(inputIndex, 0, bytes.size, presentationTimeUs, 0)
                presentationTimeUs += frameIntervalUs
            }
        }

        val bufferInfo = MediaCodec.BufferInfo()
        var outputIndex = codec.dequeueOutputBuffer(bufferInfo, 10_000)
        while (outputIndex >= 0) {
            codec.releaseOutputBuffer(outputIndex, true)
            outputIndex = codec.dequeueOutputBuffer(bufferInfo, 0)
        }

        Thread.sleep(frameIntervalUs / 1000)
    }

    codec.stop()
    codec.release()
}
