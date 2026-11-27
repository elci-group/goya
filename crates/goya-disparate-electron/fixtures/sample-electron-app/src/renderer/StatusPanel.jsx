import React, { useEffect, useState } from "react"

export function StatusPanel() {
  const [status, setStatus] = useState("idle")
  useEffect(() => {
    fetch("/api/status")
      .then((response) => response.json())
      .then((data) => setStatus(data.state))
    return () => setStatus("aborted")
  }, [status])
  return <p onClick={() => setStatus("ok")}>{status}</p>
}
