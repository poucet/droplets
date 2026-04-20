package com.simply.droplets

import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.HttpURLConnection
import java.net.Socket
import java.net.URI
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.util.Base64
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * HTTP + WebSocket client to the Droplets plugin process (localhost:9999).
 *
 * Uses only `java.base` APIs (`HttpURLConnection`, `java.net.Socket`) — Bitwig's
 * extension classloader does NOT expose `java.net.http`, so the modern HttpClient
 * throws `ClassNotFoundException` on extension init.
 *
 * All network I/O runs on a dedicated executor — never on Bitwig's controller thread.
 */
class DropletsClient(private val log: (String) -> Unit) {
    private val io = Executors.newSingleThreadExecutor { r -> Thread(r, "droplets-io").apply { isDaemon = true } }

    private val wsWanted = AtomicBoolean(false)
    @Volatile private var wsSocket: Socket? = null
    @Volatile private var wsReconnectDelayMs: Long = 1_000

    fun postProjectLayout(json: String) = post("/project_layout", json)

    fun postRenameInstance(instanceId: String, name: String) =
        post("/rename_instance", Json.encode(mapOf("instance" to instanceId, "name" to name)))

    fun setWebSocketDesired(wanted: Boolean) {
        if (wanted) {
            if (wsWanted.compareAndSet(false, true)) io.submit { openWs() }
        } else if (wsWanted.compareAndSet(true, false)) {
            closeWs()
        }
    }

    fun shutdown() {
        wsWanted.set(false)
        closeWs()
        io.shutdownNow()
    }

    // --- HTTP POST ------------------------------------------------------

    private fun post(path: String, body: String) {
        io.submit {
            try {
                val conn = URI.create("http://127.0.0.1:9999$path").toURL().openConnection() as HttpURLConnection
                conn.requestMethod = "POST"
                conn.connectTimeout = 2_000
                conn.readTimeout = 2_000
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "application/json")
                val bytes = body.toByteArray(StandardCharsets.UTF_8)
                conn.setFixedLengthStreamingMode(bytes.size)
                conn.outputStream.use { it.write(bytes) }
                val code = conn.responseCode
                log("POST $path (${bytes.size}B) → $code")
                conn.disconnect()
            } catch (e: Throwable) {
                log("POST $path failed: ${e.message}")
            }
        }
    }

    // --- WebSocket (RFC 6455) ------------------------------------------

    private fun openWs() {
        if (!wsWanted.get()) return
        var sock: Socket? = null
        try {
            sock = Socket("127.0.0.1", 9999).apply { soTimeout = 0 }
            val out = DataOutputStream(sock.getOutputStream())
            val input = DataInputStream(sock.getInputStream())

            val key = Base64.getEncoder().encodeToString(ByteArray(16).also { java.util.Random().nextBytes(it) })
            val handshake = buildString {
                append("GET /ws/controller HTTP/1.1\r\n")
                append("Host: 127.0.0.1:9999\r\n")
                append("Upgrade: websocket\r\n")
                append("Connection: Upgrade\r\n")
                append("Sec-WebSocket-Key: ").append(key).append("\r\n")
                append("Sec-WebSocket-Version: 13\r\n\r\n")
            }
            out.write(handshake.toByteArray(StandardCharsets.US_ASCII))
            out.flush()

            if (!readHandshakeResponse(input, key)) {
                sock.close()
                log("WS handshake failed — retrying in ${wsReconnectDelayMs}ms")
                scheduleReconnect()
                return
            }

            wsSocket = sock
            wsReconnectDelayMs = 1_000
            log("WS connected")
            readLoop(input)
        } catch (e: Throwable) {
            try { sock?.close() } catch (_: Throwable) {}
            wsSocket = null
            log("WS error: ${e.message} — retrying in ${wsReconnectDelayMs}ms")
            scheduleReconnect()
        }
    }

    private fun readHandshakeResponse(input: DataInputStream, clientKey: String): Boolean {
        val statusLine = readLine(input) ?: return false
        if (!statusLine.startsWith("HTTP/1.1 101")) return false
        val expected = wsAccept(clientKey)
        var seenAccept = false
        while (true) {
            val line = readLine(input) ?: return false
            if (line.isEmpty()) break
            if (line.startsWith("Sec-WebSocket-Accept:", ignoreCase = true)) {
                seenAccept = line.substringAfter(':').trim() == expected
            }
        }
        return seenAccept
    }

    private fun readLine(input: DataInputStream): String? {
        val sb = StringBuilder()
        while (true) {
            val c = input.read()
            if (c == -1) return null
            if (c == '\r'.code) continue
            if (c == '\n'.code) return sb.toString()
            sb.append(c.toChar())
        }
    }

    private fun wsAccept(clientKey: String): String {
        val magic = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
        val digest = MessageDigest.getInstance("SHA-1").digest((clientKey + magic).toByteArray(StandardCharsets.US_ASCII))
        return Base64.getEncoder().encodeToString(digest)
    }

    private fun readLoop(input: DataInputStream) {
        while (wsWanted.get()) {
            val b0 = input.read()
            if (b0 == -1) throw java.io.EOFException("server closed")
            val opcode = b0 and 0x0F
            val b1 = input.read()
            if (b1 == -1) throw java.io.EOFException("server closed")
            val masked = (b1 and 0x80) != 0
            var len: Long = (b1 and 0x7F).toLong()
            when (len) {
                126L -> len = (input.readUnsignedShort()).toLong()
                127L -> len = input.readLong()
            }
            if (masked) input.skipBytes(4)
            val payload = ByteArray(len.toInt())
            input.readFully(payload)
            when (opcode) {
                0x1 -> log("WS recv: ${String(payload, StandardCharsets.UTF_8)}")
                0x8 -> throw java.io.EOFException("server sent close")
                0x9 -> sendFrame(0xA, payload) // ping → pong
                // 0x0 continuation / 0x2 binary / 0xA pong: ignored
            }
        }
    }

    private fun sendFrame(opcode: Int, payload: ByteArray) {
        val sock = wsSocket ?: return
        val out = sock.getOutputStream()
        synchronized(sock) {
            out.write(0x80 or opcode)
            val mask = ByteArray(4).also { java.util.Random().nextBytes(it) }
            when {
                payload.size <= 125 -> out.write(0x80 or payload.size)
                payload.size <= 0xFFFF -> {
                    out.write(0x80 or 126)
                    out.write((payload.size shr 8) and 0xFF)
                    out.write(payload.size and 0xFF)
                }
                else -> {
                    out.write(0x80 or 127)
                    for (i in 7 downTo 0) out.write(((payload.size.toLong() ushr (i * 8)) and 0xFF).toInt())
                }
            }
            out.write(mask)
            val masked = ByteArray(payload.size) { i -> (payload[i].toInt() xor mask[i % 4].toInt()).toByte() }
            out.write(masked)
            out.flush()
        }
    }

    private fun closeWs() {
        val sock = wsSocket ?: return
        wsSocket = null
        try { sock.close() } catch (_: Throwable) {}
    }

    private fun scheduleReconnect() {
        if (!wsWanted.get()) return
        val delay = wsReconnectDelayMs
        wsReconnectDelayMs = (wsReconnectDelayMs * 2).coerceAtMost(30_000)
        io.submit {
            try { Thread.sleep(delay) } catch (_: InterruptedException) { return@submit }
            openWs()
        }
    }
}
