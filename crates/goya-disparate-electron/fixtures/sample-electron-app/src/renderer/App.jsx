import React, { useEffect, useState } from "react"

export default function App() {
  const [count, setCount] = useState(0)
  useEffect(() => {
    window.desktopAPI.readSettings().then((settings) => setCount(settings.start))
  }, [])
  return (
    <div className="panel">
      <button onClick={() => setCount(count + 1)}>increment</button>
      <span>{count}</span>
    </div>
  )
}
