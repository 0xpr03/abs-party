import { ref } from 'vue'

const MAX = 5

export function useEventLog() {
  const events = ref([])

  function addEvent(message, level = 'normal') {
    const time = new Date().toLocaleTimeString('en-US', { hour12: false })
    events.value = [...events.value, { time, message, level }].slice(-MAX)
  }

  return { events, addEvent }
}
