import { ref, onUnmounted } from 'vue'

export function useWebSocket(onMessage) {
  const connected = ref(false)
  let ws = null
  let pingInterval = null
  let rtt = ref(0)
  let pendingPings = {}

  function connect() {
    const protocol = location.protocol === 'https:' ? 'wss' : 'ws'
    ws = new WebSocket(`${protocol}://${location.host}/ws`)

    ws.onopen = () => {
      connected.value = true
      pingInterval = setInterval(sendPing, 5000)
    }

    ws.onclose = () => {
      connected.value = false
      clearInterval(pingInterval)
      // Reconnect after 3s
      setTimeout(connect, 3000)
    }

    ws.onerror = () => ws.close()

    ws.onmessage = (evt) => {
      let msg
      try { msg = JSON.parse(evt.data) } catch { return }

      if (msg.type === 'pong') {
        const sent = pendingPings[msg.sent_at]
        if (sent) {
          rtt.value = Date.now() - sent
          delete pendingPings[msg.sent_at]
        }
        return
      }

      onMessage(msg)
    }
  }

  function send(obj) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(obj))
    }
  }

  function sendPing() {
    const sentAt = Date.now()
    pendingPings[sentAt] = sentAt
    send({ type: 'ping', sent_at: sentAt })
  }

  function disconnect() {
    clearInterval(pingInterval)
    ws?.close()
  }

  onUnmounted(disconnect)

  return { connect, send, disconnect, connected, rtt }
}
