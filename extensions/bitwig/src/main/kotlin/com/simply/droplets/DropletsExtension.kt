package com.simply.droplets

import com.bitwig.extension.callback.StringArrayValueChangedCallback
import com.bitwig.extension.controller.ControllerExtension
import com.bitwig.extension.controller.api.ControllerHost
import com.bitwig.extension.controller.api.CursorRemoteControlsPage
import com.bitwig.extension.controller.api.Device
import com.bitwig.extension.controller.api.DeviceBank
import com.bitwig.extension.controller.api.DirectParameterValueDisplayObserver
import com.bitwig.extension.controller.api.DrumPad
import com.bitwig.extension.controller.api.DrumPadBank
import com.bitwig.extension.controller.api.RemoteControl
import com.bitwig.extension.controller.api.Track
import com.bitwig.extension.controller.api.TrackBank
import java.util.UUID

// Bank sizes are fixed at init — Bitwig only lets you allocate observation
// scaffolding during driver init. Sized generously: the cost is ~a few MB of
// JVM scaffolding and ~40k `.get()` calls per poll tick (negligible), but it
// covers every realistic project size so we don't silently truncate the
// layout. The polling-based rebuild means there's no per-slot callback cost.
private const val NUM_TRACKS = 128
private const val DEVICES_PER_TRACK = 32

// Drum pads: one bank slot per MIDI note (0-127). Using the full range with
// scroll=0 means pad index *is* the MIDI note — no offset math, no dependence
// on `scrollPosition().set()` actually being honored by the API. The extra
// JVM scaffolding (128 vs 64 slots) costs a few MB per drum track, which
// matters less than eliminating a whole class of "is the scroll active?" bugs.
private const val DRUM_PADS = 128
private const val DEVICES_PER_PAD = 4

/// Period between project-layout rebuilds. The plugin side caches the last
/// layout and suppresses re-pushes when unchanged, so running at a fixed
/// cadence costs almost nothing when the project is static. 500ms is well
/// below the threshold at which a user would notice UI lag after adding a
/// track or swapping a preset.
private const val REBUILD_PERIOD_MS = 500L

private const val REMOTE_CONTROLS_PER_PAGE = 8

// Bitwig's native Drum Machine UUID. Published in the community extension library
// (novation.commonsmk3.SpecialDevices). Used with `createBitwigDeviceMatcher` to
// narrow a filtered device bank to drum machines only — we attach the 32-pad bank
// to the filtered slot, not to every device slot (that OOMs the JVM).
private val DRUM_MACHINE_UUID: UUID = UUID.fromString("8ea97e45-0255-40fd-bc7e-94419741e9d1")

// Cached display strings matching this shape are the Droplets instance ID.
private val INSTANCE_ID_REGEX = Regex("^droplets-[0-9a-f]+$")

/**
 * Controller extension for Droplets.
 *
 * Design choice: **poll, don't subscribe.**
 *
 * Bitwig's API requires `markInterested()` at init for any value you want to
 * read later via `.get()` — that's unavoidable. But the classic pattern of
 * also attaching `addValueObserver(rebuild)` on every track/device/pad field
 * creates a large scaffolding of observers that all funnel into the same
 * `scheduleRebuild` call with a debouncer. Switching to a fixed-period timer
 * collapses that whole layer into a single line.
 *
 * What still needs an observer: `DirectParameterValueDisplayObserver` on every
 * device, because that's the only API Bitwig exposes for reading direct
 * parameter values — and we scan display strings for the Droplets instance-ID
 * pattern. That observer just fills a cache; the periodic rebuild reads it.
 */
class DropletsExtension(
    definition: DropletsExtensionDefinition,
    private val host: ControllerHost,
) : ControllerExtension(definition, host) {

    private lateinit var client: DropletsClient
    private lateinit var trackBank: TrackBank
    private val renamedInstances = HashSet<String>()

    /** Instance IDs we've already announced in the log. Keeps detection logs non-spammy. */
    private val announcedInstances = HashSet<String>()

    /** Per-device param-id → displayed-value cache. Scanned for the Droplets ID pattern. */
    private val deviceDisplays = HashMap<Device, MutableMap<String, String>>()

    /** Per-track device bank, created at init. */
    private val trackDeviceBanks = HashMap<Track, DeviceBank>()

    /** Per-track drum pad bank, taken from a device-matcher-filtered bank (1 drum machine per track). */
    private val trackDrumPadBanks = HashMap<Track, DrumPadBank>()

    /** Per-pad nested device bank, created at init — used to read the loaded sample name. */
    private val padDeviceBanks = HashMap<DrumPad, DeviceBank>()

    /** Per-track primary-instrument Remote Controls Page 1 cursor, created at init. */
    private val trackRemotePages = HashMap<Track, CursorRemoteControlsPage>()

    /**
     * Mirror of `host.println` to a file so logs are visible without the
     * Bitwig Controller Script Console (which some Bitwig versions don't
     * expose a menu entry for). Path matches the plugin side's convention
     * so both logs can be tailed from the same dir.
     */
    private val logFile: java.io.File =
        java.io.File(System.getProperty("user.home"), "droplets_bitwig.log")

    override fun init() {
        client = DropletsClient(::log)

        trackBank = host.createTrackBank(NUM_TRACKS, 0, 0)
        for (t in 0 until NUM_TRACKS) wireTrack(trackBank.getItemAt(t) as Track)

        scheduleNextRebuild()
        log("Droplets extension initialized — watching $NUM_TRACKS tracks, polling every ${REBUILD_PERIOD_MS}ms")
    }

    override fun flush() {}

    override fun exit() {
        client.shutdown()
    }

    // --- markInterested scaffolding (all at init — Bitwig requires it) ---

    private fun wireTrack(track: Track) {
        track.exists().markInterested()
        track.position().markInterested()
        track.name().markInterested()

        val devices = track.createDeviceBank(DEVICES_PER_TRACK)
        trackDeviceBanks[track] = devices
        for (d in 0 until DEVICES_PER_TRACK) wireDevice(devices.getItemAt(d) as Device)

        // Drum-machine-filtered bank (1 slot per track). The filter keeps the drum pad
        // bank attached to at most one device per track — 128 pad proxies × 4 nested
        // devs = 512 pad proxies per track, rather than 512 × 32 × 4 = 65K for the
        // unfiltered case.
        val drumMatcher = host.createBitwigDeviceMatcher(DRUM_MACHINE_UUID)
        val drumBank = track.createDeviceBank(1)
        drumBank.setDeviceMatcher(drumMatcher)
        val drumDev = drumBank.getItemAt(0) as Device
        drumDev.exists().markInterested()
        val padBank = drumDev.createDrumPadBank(DRUM_PADS)
        // Iterate pads by position, not by "existing" shortlist, so pad bank
        // index + scrollPosition = MIDI note holds for every slot.
        padBank.setSkipDisabledItems(false)
        // Read-access to the current scroll: we don't set it, we just need
        // to know where Bitwig's window currently starts so we can translate
        // bank-index → MIDI note correctly regardless of user scrolling.
        padBank.scrollPosition().markInterested()
        trackDrumPadBanks[track] = padBank
        for (p in 0 until DRUM_PADS) wirePad(padBank.getItemAt(p) as DrumPad)

        // Primary-instrument Remote Controls Page 1. Follows the primary
        // instrument automatically, so swapping the preset refreshes the names
        // on the next poll tick.
        val remote = track.createCursorRemoteControlsPage(REMOTE_CONTROLS_PER_PAGE)
        trackRemotePages[track] = remote
        remote.pageCount().markInterested()
        for (i in 0 until REMOTE_CONTROLS_PER_PAGE) {
            val rc = remote.getParameter(i) as RemoteControl
            rc.exists().markInterested()
            rc.name().markInterested()
        }
    }

    private fun wireDevice(device: Device) {
        device.exists().markInterested()
        device.isPlugin().markInterested()
        device.hasDrumPads().markInterested()
        device.name().markInterested()
        device.presetName().markInterested()
        device.deviceType().markInterested()

        // Direct-parameter observers are the ONLY way to read direct-parameter
        // values in the Bitwig API. We need them to find the Droplets
        // instance-ID param (display value matches `droplets-[hex]+`). No
        // CLAP device matcher exists; the VST3 UID isn't pinned on the Rust
        // side. So we pay the observer cost on every device.
        //
        // These observers populate a cache only — the periodic poll picks up
        // changes on its next tick. The holder pattern resolves the circular
        // dep: Bitwig requires the ID observer to be registered BEFORE the
        // display observer.
        deviceDisplays[device] = HashMap()
        val displayObsHolder = arrayOfNulls<DirectParameterValueDisplayObserver>(1)

        device.addDirectParameterIdObserver(StringArrayValueChangedCallback { raw ->
            (raw as? Array<*>)
                ?.filterIsInstance<String>()
                ?.toTypedArray()
                ?.let { displayObsHolder[0]?.setObservedParameterIds(it) }
        })
        displayObsHolder[0] = device.addDirectParameterValueDisplayObserver(32) { id, value ->
            if (id != null && value != null) {
                deviceDisplays[device]?.put(id, value)
            }
        }
    }

    private fun wirePad(pad: DrumPad) {
        pad.exists().markInterested()
        pad.name().markInterested()
        val bank = pad.createDeviceBank(DEVICES_PER_PAD)
        padDeviceBanks[pad] = bank
        for (d in 0 until DEVICES_PER_PAD) {
            val dev = bank.getItemAt(d) as Device
            dev.exists().markInterested()
            dev.name().markInterested()
            dev.presetName().markInterested()
        }
    }

    // --- Periodic rebuild + POST ----------------------------------------

    private fun scheduleNextRebuild() {
        host.scheduleTask({
            try {
                rebuildAndPost()
            } catch (e: Throwable) {
                log("rebuild failed: ${e.message}")
            } finally {
                scheduleNextRebuild()
            }
        }, REBUILD_PERIOD_MS)
    }

    private fun rebuildAndPost() {
        val tracks = ArrayList<Map<String, Any?>>()
        var trackCount = 0
        var dropletsCount = 0
        for (t in 0 until NUM_TRACKS) {
            val track = trackBank.getItemAt(t) as Track
            if (!track.exists().get()) continue
            trackCount++
            val trackName: String = track.name().get()
            val trackDevices = trackDeviceBanks[track] ?: continue

            var instanceId: String? = null
            for (d in 0 until DEVICES_PER_TRACK) {
                val device = trackDevices.getItemAt(d) as Device
                if (!device.exists().get()) continue
                if (instanceId == null) findInstanceId(device)?.let { instanceId = it }
            }

            if (instanceId != null) {
                dropletsCount++
                val id = instanceId!!
                if (announcedInstances.add(id)) {
                    log("detected Droplets instance '$id' on track '$trackName'")
                }
                if (renamedInstances.add(id)) {
                    log("→ rename $id to '$trackName'")
                    client.postRenameInstance(id, trackName)
                }
            }

            val trackJson = linkedMapOf<String, Any?>(
                "track_name" to trackName,
                "droplets_instance_id" to instanceId,
                "primary_device" to pickPrimaryDevice(track),
            )
            if (instanceId != null) {
                trackJson["remote_controls"] = encodeRemoteControls(track)
            }
            tracks.add(trackJson)
        }

        val hasAnyDroplets = dropletsCount > 0
        client.setWebSocketDesired(hasAnyDroplets)
        if (hasAnyDroplets) {
            client.postProjectLayout(Json.encode(mapOf("tracks" to tracks)))
        }
    }

    /** Pick the track's primary sound source: drum machine if present, else
     *  the first existing instrument on the main chain. Effects and unknown
     *  devices are skipped. Returns null for effects-only tracks. */
    private fun pickPrimaryDevice(track: Track): Map<String, Any?>? {
        val drumPadBank = trackDrumPadBanks[track]
        val drumDev = drumPadBank?.let { drumDeviceForPadBank(track) }
        if (drumDev != null && drumDev.exists().get()) {
            return encodeDrumMachine(drumDev.name().get(), drumPadBank)
        }

        val devices = trackDeviceBanks[track] ?: return null
        for (d in 0 until DEVICES_PER_TRACK) {
            val device = devices.getItemAt(d) as Device
            if (!device.exists().get()) continue
            if (device.deviceType().get() == "instrument") {
                return encodeInstrument(device)
            }
        }
        return null
    }

    private fun drumDeviceForPadBank(track: Track): Device? {
        // The drum-machine-filtered bank is attached to this track via the
        // matcher in wireTrack. We stored the pad bank but not the device
        // itself — the device is accessible via the same bank we built the
        // pad bank from. Simpler: walk the main device bank for a device
        // whose hasDrumPads is true.
        val devices = trackDeviceBanks[track] ?: return null
        for (d in 0 until DEVICES_PER_TRACK) {
            val device = devices.getItemAt(d) as Device
            if (device.exists().get() && device.hasDrumPads().get()) return device
        }
        return null
    }

    private fun encodeInstrument(device: Device): Map<String, Any?> {
        val out = linkedMapOf<String, Any?>(
            "type" to "instrument",
            "name" to device.name().get(),
        )
        if (!device.isPlugin().get()) out["vendor"] = "Bitwig"
        val preset = device.presetName().get().takeIf { it.isNotBlank() }
        if (preset != null) out["preset_name"] = preset
        return out
    }

    private fun encodeDrumMachine(name: String, padBank: DrumPadBank): Map<String, Any?> {
        val pads = ArrayList<Map<String, Any?>>()
        // Read the bank's current scroll — translating bank-index → MIDI note
        // must use the actual scroll, not a constant we set at init (that
        // `set()` call isn't always honored by the API). `setSkipDisabledItems(false)`
        // at init guarantees bank index `p` maps to note `scroll + p`.
        val scroll = padBank.scrollPosition().get()
        for (p in 0 until DRUM_PADS) {
            val pad = padBank.getItemAt(p) as DrumPad
            if (!pad.exists().get()) continue
            val padJson = linkedMapOf<String, Any?>(
                "note" to (scroll + p),
                "name" to pad.name().get(),
            )
            padSampleName(pad)?.let { padJson["sample_name"] = it }
            pads.add(padJson)
        }
        return linkedMapOf("type" to "drum_machine", "name" to name, "pads" to pads)
    }

    /** Pull the loaded sample / preset name from the pad's first nested device.
     *  For Bitwig's Sampler this is the audio file name ("kick_808.wav"). */
    private fun padSampleName(pad: DrumPad): String? {
        val bank = padDeviceBanks[pad] ?: return null
        for (i in 0 until DEVICES_PER_PAD) {
            val nd = bank.getItemAt(i) as Device
            if (!nd.exists().get()) continue
            val preset = nd.presetName().get().takeIf { it.isNotBlank() }
            if (preset != null) return preset
        }
        return null
    }

    /** Snapshot Remote Controls Page 1 as `[{index, name}]`, filtering out
     *  slots with no existing param or no name. */
    private fun encodeRemoteControls(track: Track): List<Map<String, Any?>> {
        val remote = trackRemotePages[track] ?: return emptyList()
        val out = ArrayList<Map<String, Any?>>()
        for (i in 0 until REMOTE_CONTROLS_PER_PAGE) {
            val rc = remote.getParameter(i) as RemoteControl
            if (!rc.exists().get()) continue
            val name = rc.name().get()
            if (name.isBlank()) continue
            out.add(linkedMapOf("index" to i, "name" to name))
        }
        return out
    }

    private fun findInstanceId(device: Device): String? {
        val displays = deviceDisplays[device] ?: return null
        for (v in displays.values) if (INSTANCE_ID_REGEX.matches(v)) return v
        return null
    }

    private fun log(msg: String) {
        host.println("[droplets] $msg")
        // Also tee to a file so `tail -f ~/droplets_bitwig.log` works even
        // when the Controller Script Console isn't reachable from the menu.
        try {
            val ts = java.time.LocalTime.now().toString()
            logFile.appendText("[$ts] $msg\n")
        } catch (_: Throwable) {
            // Best-effort — never let logging break the extension.
        }
    }
}
