package com.danchor.app

import java.net.InetAddress

/** Where a [DiscoveredDesktop] entry came from, so a reload can selectively
 * refresh only the entries it's actually able to re-verify (scan results)
 * without touching entries that are already managed another way (mDNS is
 * reactive on its own; manual entries are the user's explicit choice). */
enum class DesktopSource {
    Mdns,
    Scan,
    Manual,
}

data class DiscoveredDesktop(
    val name: String,
    val host: InetAddress,
    val port: Int,
    val source: DesktopSource,
)

sealed interface PingState {
    data object Idle : PingState

    data object InFlight : PingState

    // deviceName/Id/Icon come from the responder's Pong reply (see
    // Networking.kt's pingDevice) - every discovery path converges on a
    // Ping/Pong round trip, so this is how a real device name replaces a
    // bare IP address regardless of whether mDNS itself worked.
    data class Success(val rttMs: Long, val deviceName: String, val deviceId: String, val deviceIcon: String) : PingState

    data class Failure(val message: String) : PingState
}

/** Result of attempting to establish a Noise-encrypted session with a
 * desktop (see Networking.kt's establishSecureSession) - deliberately kept
 * separate from [PingState], which only ever reflects the plain-text
 * discovery ping. A [Failed] secure attempt (e.g. no/mismatched trust
 * secret configured on the desktop yet) doesn't affect whether the plain
 * ping itself succeeded. */
sealed interface SecureResult {
    data class Secured(val rttMs: Long) : SecureResult

    data class Failed(val message: String) : SecureResult
}

/** Which transport(s) a [ConnectionListItem] is currently shown as reachable
 * over. Derived from the tablet's overall link state (USB tethering active /
 * WiFi connected) rather than per-device dual-interface probing - see
 * conventions.toon for why. */
enum class ConnectionType { CABLE, WIFI }

/** Whether a [ConnectionListItem] is present in this session's live
 * discovery/scan results right now, or only known from a past session. */
enum class ConnectionStatus { ONLINE, OFFLINE }

/** A connection that survived past the current session - written the first
 * time a [PingState.Success] happens for a [DiscoveredDesktop], read back on
 * next launch so it still shows up (as OFFLINE) even before rediscovery. */
data class SavedConnection(
    val id: String,
    val name: String,
    val host: String,
    val port: Int,
    val lastConnectedAtMs: Long,
)

/** The merged view [ConnectionCard] actually renders: live [DiscoveredDesktop]
 * entries and persisted [SavedConnection] entries collapsed into one list. */
data class ConnectionListItem(
    val id: String,
    val name: String,
    val host: String,
    val port: Int,
    val status: ConnectionStatus,
    val types: Set<ConnectionType>,
    val pingState: PingState,
    // Whether this id has ever been successfully connected to before (i.e.
    // exists in savedConnections) - "unknown" isn't just OFFLINE, it also
    // covers a live ONLINE device nobody has ever pinged yet.
    val isKnown: Boolean,
)
