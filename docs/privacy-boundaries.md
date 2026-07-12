# Privacy boundaries

The gateway only processes requests routed through its local Base URL. It cannot detect content sent directly to an upstream provider, via a browser page, a hard-coded endpoint, or an unsupported protocol.

Strict policy fails closed for detected credentials. Other supported text is transformed only in documented protocol fields. Unknown JSON fields and signed Anthropic thinking blocks are not inspected or rebuilt. Detection is heuristic plus deterministic validation; users must review high-risk material.
