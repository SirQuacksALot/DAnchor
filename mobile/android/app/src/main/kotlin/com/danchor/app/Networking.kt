package com.danchor.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.danchor_ffi.NoiseHandshake
import uniffi.danchor_ffi.decodeEncrypted
import uniffi.danchor_ffi.decodeHandshake
import uniffi.danchor_ffi.decodePong
import uniffi.danchor_ffi.defaultPort
import uniffi.danchor_ffi.encodeEncrypted
import uniffi.danchor_ffi.encodeHandshake
import uniffi.danchor_ffi.encodePing
import uniffi.danchor_ffi.generateNoiseKeypair
import uniffi.danchor_ffi.scanCandidates
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.SocketTimeoutException

/** The currently active USB-tethering network, if any. Not a
 * ConnectivityManager.NetworkCallback subscription - see conventions on why
 * (never fires on Sebastian's Galaxy Tab S9 / One UI despite a genuinely
 * matching request+network) - callers that need to react to it appearing
 * poll this instead. */
fun findUsbNetwork(connectivityManager: ConnectivityManager): Network? =
    connectivityManager.allNetworks.firstOrNull { network ->
        connectivityManager.getNetworkCapabilities(network)?.hasTransport(NetworkCapabilities.TRANSPORT_USB) == true
    }

/** Whether the tablet currently has a working WiFi link at all - used to
 * show a WLAN chip on connection cards as "available as a fallback", not to
 * verify that a specific desktop is reachable over it (see conventions.toon
 * for why per-device dual-interface probing was deliberately skipped). */
fun isWifiConnected(connectivityManager: ConnectivityManager): Boolean =
    connectivityManager.allNetworks.any { network ->
        connectivityManager.getNetworkCapabilities(network)?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true
    }

// This device's own IPv4 address and subnet prefix length to scan, or null
// if unavailable. Prefers a USB-tethering network over `activeNetwork`:
// `activeNetwork` resolves to whichever network Android considers "best"
// (usually whichever has internet access), which on a USB-tethered-but-
// still-WiFi-connected device is often WiFi, not the USB link to the
// desktop we actually want to scan.
private fun getLocalIpv4WithPrefix(context: Context): Pair<String, Int>? {
    val connectivityManager = context.getSystemService(ConnectivityManager::class.java) ?: return null

    val network = findUsbNetwork(connectivityManager) ?: connectivityManager.activeNetwork ?: return null

    val linkProperties = connectivityManager.getLinkProperties(network) ?: return null
    val linkAddress = linkProperties.linkAddresses.firstOrNull { it.address is Inet4Address } ?: return null
    val ip = linkAddress.address.hostAddress ?: return null
    return ip to linkAddress.prefixLength
}

suspend fun pingDevice(
    context: Context,
    host: String,
    port: Int,
): PingState =
    withContext(Dispatchers.IO) {
        try {
            DatagramSocket().use { socket ->
                socket.soTimeout = 3000
                val sequence = 1u
                val sentAt = System.currentTimeMillis()
                val pingBytes = encodePing(sequence, sentAt.toULong())

                val address = InetAddress.getByName(host)
                val packet = DatagramPacket(pingBytes, pingBytes.size, address, port)
                socket.send(packet)

                val buf = ByteArray(2048)
                val reply = DatagramPacket(buf, buf.size)
                socket.receive(reply)

                val pong =
                    decodePong(reply.data.copyOf(reply.length))
                        ?: return@withContext PingState.Failure(context.getString(R.string.error_unexpected_reply))

                val rtt = System.currentTimeMillis() - pong.timestampMs.toLong()
                PingState.Success(rtt, pong.deviceName, pong.deviceId, pong.deviceIcon)
            }
        } catch (e: Exception) {
            PingState.Failure(e.message ?: context.getString(R.string.error_unknown))
        }
    }

// Unicast-probes every candidate host in the local subnet with a Ping,
// collecting whichever ones reply with a Pong within the deadline. This is
// the fallback for when mDNS discovery doesn't work.
suspend fun scanSubnetForDesktops(context: Context): List<DiscoveredDesktop> =
    withContext(Dispatchers.IO) {
        val (localIp, prefixLen) = getLocalIpv4WithPrefix(context) ?: return@withContext emptyList()
        val candidates = scanCandidates(localIp, prefixLen.toUByte())
        if (candidates.isEmpty()) return@withContext emptyList()

        val port = defaultPort().toInt()
        val found = mutableListOf<DiscoveredDesktop>()

        DatagramSocket().use { socket ->
            socket.soTimeout = 200

            val sentAt = System.currentTimeMillis()
            val pingBytes = encodePing(1u, sentAt.toULong())
            for (ip in candidates) {
                socket.send(DatagramPacket(pingBytes, pingBytes.size, InetAddress.getByName(ip), port))
            }

            val deadline = System.currentTimeMillis() + 1500
            val buf = ByteArray(2048)
            while (System.currentTimeMillis() < deadline) {
                try {
                    val reply = DatagramPacket(buf, buf.size)
                    socket.receive(reply)
                    val pong = decodePong(reply.data.copyOf(reply.length)) ?: continue
                    val host = reply.address
                    if (found.none { it.host == host }) {
                        val name = pong.deviceName.ifBlank { host.hostAddress ?: host.toString() }
                        found.add(DiscoveredDesktop(name, host, port, DesktopSource.Scan))
                    }
                } catch (e: SocketTimeoutException) {
                    // No packet within this short poll interval; keep going until the deadline.
                }
            }
        }

        found
    }

fun hexToBytes(hex: String): ByteArray? {
    if (hex.isEmpty() || hex.length % 2 != 0) return null
    return try {
        ByteArray(hex.length / 2) { i -> hex.substring(i * 2, i * 2 + 2).toInt(16).toByte() }
    } catch (e: NumberFormatException) {
        null
    }
}

// Establishes a Noise-encrypted session with a desktop, authenticated by
// the household trust secret (AppPreferences.pairingSecret), and proves it
// works end to end by sending one Ping through it - deliberately separate
// from pingDevice above, which stays the plain-text discovery/RTT check
// exactly as before. This session lives only for the duration of this one
// round trip; Module 2 (screen mirroring) is what will need a channel that
// outlives a single check. Only the PSK-authenticated pattern is supported
// - a desktop with no matching trust secret configured simply rejects the
// handshake (see danchor_core::transport::ConnectionRegistry).
suspend fun establishSecureSession(
    context: Context,
    host: String,
    port: Int,
    secretHex: String,
): SecureResult =
    withContext(Dispatchers.IO) {
        try {
            val psk = hexToBytes(secretHex) ?: return@withContext SecureResult.Failed(
                context.getString(R.string.error_unknown),
            )

            DatagramSocket().use { socket ->
                socket.soTimeout = 3000
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

                val message1 =
                    handshake.writeNext()
                        ?: return@withContext SecureResult.Failed(context.getString(R.string.error_unknown))
                sendHandshake(message1)

                val message2 =
                    receiveHandshake()
                        ?: return@withContext SecureResult.Failed(context.getString(R.string.error_unexpected_reply))
                if (!handshake.readNext(message2)) {
                    return@withContext SecureResult.Failed(context.getString(R.string.error_unexpected_reply))
                }

                // Message 3 is Noise_XX's last message - no reply is expected.
                val message3 =
                    handshake.writeNext()
                        ?: return@withContext SecureResult.Failed(context.getString(R.string.error_unknown))
                sendHandshake(message3)

                val channel =
                    handshake.intoSession()
                        ?: return@withContext SecureResult.Failed(context.getString(R.string.error_unknown))

                val sentAt = System.currentTimeMillis()
                val plaintext = encodePing(1u, sentAt.toULong())
                val ciphertext =
                    channel.encrypt(0u, plaintext)
                        ?: return@withContext SecureResult.Failed(context.getString(R.string.error_unknown))
                val outer = encodeEncrypted(0u, ciphertext)
                socket.send(DatagramPacket(outer, outer.size, address, port))

                val buf = ByteArray(2048)
                val reply = DatagramPacket(buf, buf.size)
                socket.receive(reply)
                val encryptedReply =
                    decodeEncrypted(reply.data.copyOf(reply.length))
                        ?: return@withContext SecureResult.Failed(context.getString(R.string.error_unexpected_reply))
                val replyPlaintext =
                    channel.decrypt(encryptedReply.sequence, encryptedReply.ciphertext)
                        ?: return@withContext SecureResult.Failed(context.getString(R.string.error_unexpected_reply))
                val pong =
                    decodePong(replyPlaintext)
                        ?: return@withContext SecureResult.Failed(context.getString(R.string.error_unexpected_reply))

                SecureResult.Secured(System.currentTimeMillis() - pong.timestampMs.toLong())
            }
        } catch (e: Exception) {
            SecureResult.Failed(e.message ?: context.getString(R.string.error_unknown))
        }
    }
