import { useEffect, useState } from "react";
import { desktopApi, type DesktopApi } from "./desktop-api";
import type { GatewayStatus, ProfileSummary } from "./model";

export function App({
  api = desktopApi,
}: {
  api?: DesktopApi;
  initialLanguage?: string;
}) {
  const [status, setStatus] = useState<GatewayStatus>({ running: false });
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);

  useEffect(() => {
    void api.gatewayStatus().then(setStatus).catch(() => undefined);
    void api.listProfiles().then(setProfiles).catch(() => undefined);
  }, [api]);

  return (
    <main>
      <header>
        <div>
          <h1>LLM Privacy Gateway</h1>
          <p>Keep sensitive information local before model requests leave your device.</p>
        </div>
        <button disabled={profiles.length === 0} type="button">
          {status.running ? "Stop protection" : "Start protection"}
        </button>
      </header>
      <section aria-labelledby="first-run-title">
        <h2 id="first-run-title">Connect your first model</h2>
        <ol>
          <li>Add model connection</li>
          <li>Copy local settings</li>
          <li>Start protection</li>
        </ol>
        <form aria-label="Model connection">
          <label>
            Connection name
            <input name="name" />
          </label>
        </form>
      </section>
    </main>
  );
}
