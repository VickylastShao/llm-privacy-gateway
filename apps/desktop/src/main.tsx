import React, { FormEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createRoot } from "react-dom/client";
import "./styles.css";

type Page = "setup" | "dashboard" | "connections" | "policies" | "console" | "activity";
type GatewayStatus = { running: boolean };
type ProfileSummary = { id: string; name: string; protocol: string; upstream_url: string; policy: string };
type CreatedProfile = { profile: ProfileSummary; local_base_url: string; local_key: string };
const pages: Array<[Page, string]> = [["setup", "Setup"], ["dashboard", "Dashboard"], ["connections", "Connections"], ["policies", "Policies"], ["console", "Test Console"], ["activity", "Activity"]];

function App() {
  const [page, setPage] = useState<Page>("setup");
  const [status, setStatus] = useState<GatewayStatus>({ running: false });
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [created, setCreated] = useState<CreatedProfile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const loadProfiles = async () => setProfiles(await invoke<ProfileSummary[]>("list_profiles"));
  useEffect(() => {
    void invoke<GatewayStatus>("gateway_status").then(setStatus).catch(() => setError("Desktop runtime is unavailable in browser preview."));
    void loadProfiles().catch(() => undefined);
  }, []);
  const toggleGateway = async () => {
    try {
      const command = status.running ? "stop_gateway" : "start_gateway";
      setStatus(await invoke<GatewayStatus>(command));
      setError(null);
    } catch (cause) {
      setError(String(cause));
    }
  };
  const onCreated = (profile: CreatedProfile) => {
    setCreated(profile);
    setProfiles((current) => [...current, profile.profile]);
    setPage("dashboard");
  };
  return <main><aside><h1>LLM Privacy Gateway</h1>{pages.map(([id, label]) => <button className={id === page ? "active" : ""} onClick={() => setPage(id)} key={id}>{label}</button>)}</aside><section><header><h2>{pages.find(([id]) => id === page)?.[1]}</h2><button onClick={() => void toggleGateway()}>{status.running ? "Stop gateway" : "Start gateway"}</button></header>{(page === "setup" || page === "connections") ? <Connections profiles={profiles} onCreated={onCreated} onError={setError} /> : <p>{content(page)}</p>}{created && <LocalConfiguration profile={created} />}{error && <p className="notice">{error}</p>}<p className="notice">Prompts, provider keys, and pseudonym mappings are never rendered by this control plane after submission.</p></section></main>;
}

function Connections({ profiles, onCreated, onError }: { profiles: ProfileSummary[]; onCreated: (profile: CreatedProfile) => void; onError: (error: string) => void }) {
  const upstreamKey = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [protocol, setProtocol] = useState("openai");
  const [upstreamUrl, setUpstreamUrl] = useState("https://api.openai.com/v1");
  const [policy, setPolicy] = useState("strict");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const key = upstreamKey.current?.value ?? "";
    try {
      const created = await invoke<CreatedProfile>("create_profile", { draft: { name, protocol, upstreamUrl, upstreamKey: key, policy } });
      if (upstreamKey.current) upstreamKey.current.value = "";
      onError("");
      onCreated(created);
    } catch (cause) {
      if (upstreamKey.current) upstreamKey.current.value = "";
      onError(String(cause));
    }
  };
  return <><form onSubmit={(event) => void submit(event)}><label>Profile name<input value={name} onChange={(event) => setName(event.target.value)} required /></label><label>Protocol<select value={protocol} onChange={(event) => { const value = event.target.value; setProtocol(value); setUpstreamUrl(value === "anthropic" ? "https://api.anthropic.com" : "https://api.openai.com/v1"); }}><option value="openai">OpenAI-compatible</option><option value="anthropic">Anthropic Messages</option></select></label><label>Upstream Base URL<input value={upstreamUrl} onChange={(event) => setUpstreamUrl(event.target.value)} required /></label><label>Upstream API key<input ref={upstreamKey} type="password" autoComplete="off" required /></label><label>Policy<select value={policy} onChange={(event) => setPolicy(event.target.value)}><option value="strict">Strict</option><option value="balanced">Balanced</option><option value="monitor">Monitor</option></select></label><button type="submit">Create local profile</button></form>{profiles.length > 0 && <ul>{profiles.map((profile) => <li key={profile.id}>{profile.name} · {profile.protocol} · {profile.upstream_url}</li>)}</ul>}</>;
}

function LocalConfiguration({ profile }: { profile: CreatedProfile }) {
  return <article><h3>Copy this local configuration now</h3><p>It is shown only for the newly created profile.</p><code>Base URL: {profile.local_base_url}</code><code>API Key: {profile.local_key}</code></article>;
}

function content(page: Page) {
  const copy: Record<Page, string> = { setup: "Create a local profile to begin.", dashboard: "Gateway status and metadata-only activity overview.", connections: "Configure OpenAI or Anthropic endpoints. HTTP is allowed only for loopback testing.", policies: "Strict mode blocks credentials and pseudonymizes supported sensitive text.", console: "Preview transformations without persisting prompt content or secret mappings.", activity: "View endpoint, model, status, latency, and category counts only." };
  return copy[page];
}

createRoot(document.getElementById("root")!).render(<App />);
