import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";

type Page = "setup" | "connections" | "policies" | "console" | "activity";
const pages: Array<[Page, string]> = [["setup", "Setup"], ["connections", "Connections"], ["policies", "Policies"], ["console", "Test Console"], ["activity", "Activity"]];

function App() {
  const [page, setPage] = useState<Page>("setup");
  return <main><aside><h1>LLM Privacy Gateway</h1>{pages.map(([id, label]) => <button className={id === page ? "active" : ""} onClick={() => setPage(id)} key={id}>{label}</button>)}</aside><section><h2>{pages.find(([id]) => id === page)?.[1]}</h2><p>{page === "setup" ? "Create a local profile. Upstream secrets remain in the operating-system credential store." : "This page exposes metadata only; prompts, provider keys, and pseudonym mappings are never rendered."}</p></section></main>;
}
createRoot(document.getElementById("root")!).render(<App />);
