package com.simply.droplets

import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.net.http.WebSocket
import java.time.Duration
import java.util.concurrent.CompletionStage
import java.util.concurrent.atomic.AtomicBoolean

/**
 * HTTP + WebSocket client to the Droplets plugin process (localhost:9999).
 * All I/O happens on a dedicated HttpClient executor — never on the Bitwig controller thread.
 */
class DropletsClient(private val log: (String) -> Unit) {
    private val http: HttpClient = HttpClient.newBuilder()
        .connectTimeout(Duration.ofSeconds(2))
        .build()

    private val wsAlive = AtomicBoolean(false)
    private var ws: WebSocket? = null
    @Volatile private var wsReconnectDelayMs: Long = 1_000

    fun postProjectLayout(json: String) = post("/project_layout", json)
    fun postRenameInstance(instanceId: String, name: String) =
        post("/rename_instance", Json.encode(mapOf("instance" to instanceId, "name" to name)))

    private fun post(path: String, body: String) {
        val req = HttpRequest.newBuilder(URI.create("http://127.0.0.1:9999$path"))
            .timeout(Duration.ofSeconds(2))
            .header("Content-Type", "application/json")
            .POST(HttpRequest.BodyPublishers.ofString(body))
            .build()
        http.sendAsync(req, HttpResponse.BodyHandlers.ofString())
            .whenComplete { resp, err ->
                when {
                    err != null -> log("POST $path failed: ${err.message}")
                    resp.statusCode() !in 200..299 -> log("POST $path → ${resp.statusCode()} ${resp.body()}")
                }
            }
    }

    fun connectWebSocket() {
        if (!wsAlive.compareAndSet(false, true)) return
        openWs()
    }

    fun closeWebSocket() {
        wsAlive.set(false)
        ws?.sendClose(WebSocket.NORMAL_CLOSURE, "bye")
        ws = null
    }

    private fun openWs() {
        if (!wsAlive.get()) return
        http.newWebSocketBuilder()
            .buildAsync(URI.create("ws://127.0.0.1:9999/ws/controller"), wsListener)
            .whenComplete { socket, err ->
                if (err != null) {
                    log("WS connect failed: ${err.message} — retrying in ${wsReconnectDelayMs}ms")
                    scheduleReconnect()
                } else {
                    log("WS connected")
                    ws = socket
                    wsReconnectDelayMs = 1_000
                }
            }
    }

    private fun scheduleReconnect() {
        Thread.ofVirtual().start {
            try { Thread.sleep(wsReconnectDelayMs) } catch (_: InterruptedException) { return@start }
            wsReconnectDelayMs = (wsReconnectDelayMs * 2).coerceAtMost(30_000)
            openWs()
        }
    }

    private val wsListener = object : WebSocket.Listener {
        override fun onText(webSocket: WebSocket, data: CharSequence, last: Boolean): CompletionStage<*>? {
            log("WS recv: $data")
            webSocket.request(1)
            return null
        }

        override fun onError(webSocket: WebSocket, error: Throwable) {
            log("WS error: ${error.message}")
            scheduleReconnect()
        }

        override fun onClose(webSocket: WebSocket, statusCode: Int, reason: String): CompletionStage<*>? {
            log("WS closed: $statusCode $reason")
            if (wsAlive.get()) scheduleReconnect()
            return null
        }
    }
}
