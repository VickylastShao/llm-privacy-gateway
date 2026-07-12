import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createRoot } from "react-dom/client";
import "./styles.css";

type Page = "setup" | "dashboard" | "connections" | "policies" | "console" | "activity";
type GatewayStatus = { running: boolean };
const pages: Array<[Page, string]> = [["setup", "Setup"], ["dashboard", "Dashboard"], ["connections", "Connections"], ["policies", "Policies"], ["console", "Test Console"], ["activity", "Activity"]];

function App() {
  const [page, setPage] = useState<Page>("setup");
  const [status, setStatus] = useState<GatewayStatus>({ running: false });
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { void invoke<GatewayStatus>("gateway_status").then(setStatus).catch(() => setError("Desktop runtime is unavailable in browser preview.")); }, []);
  const toggleGateway = async () => { const command = status.running ? "stop_gateway" : "start_gateway"; setStatus(await invoke<GatewayStatus>(command)); };
  return <main><aside><h1>LLM Privacy Gateway</h1>{pages.map(([id, label]) => <button className={id === page ? "active" : ""} onClick={() => setPage(id)} key={id}>{label}</button>)}</aside><section><header><h2>{pages.find(([id]) => id === page)?.[1]}</h2><button onClick={() => void toggleGateway()}>{status.running ? "Stop gateway" : "Start gateway"}</button></header><p>{content(page)}</p><p className="notice">{error ?? "Prompts, provider keys, and pseudonym mappings are never rendered by this control plane."}</p></section></main>;
}

function content(page: Page) {
  const copy: Record<Page, string> = { setup: "Create local profiles; upstream credentials are stored outside UI state.", dashboard: "Gateway status and metadata-only activity overview.", connections: "Configure OpenAI or Anthropic endpoints. HTTP is allowed only for loopback testing.", policies: "Strict mode blocks credentials and pseudonymizes supported sensitive text.", console: "Preview transformations without persisting prompt content or secret mappings.", activity: "View endpoint, model, status, latency, and category counts only." };
  return copy[page];
}

createRoot(document.getElementById("root")!).render(<App />);
