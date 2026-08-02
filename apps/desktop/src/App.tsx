import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function App() {
  const [version, setVersion] = useState<string>("…");

  useEffect(() => {
    invoke<string>("core_version").then(setVersion).catch(console.error);
  }, []);

  return (
    <main className="shell">
      <h1>Kavka</h1>
      <p>The modern Kafka client. Core v{version}</p>
      <p className="hint">
        Phase 0 scaffold — see <code>docs/ROADMAP.md</code>.
      </p>
    </main>
  );
}
