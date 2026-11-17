# Comparable foundations: research snapshot 2026-11-15

## What already exists
These are documented capabilities, not independently benchmarked quality or security guarantees. Current docs evolve; pin exact tested package versions before implementation.

| Foundation | Primary-source capability | WraithKit tradeoff / recommendation |
|---|---|---|
| Pydantic AI | Typed dependencies and outputs, provider integrations, tool schemas, test substitution and observability. [Overview](https://pydantic.dev/docs/ai/overview/) | Best starting fit for two typed Python apps. Use its model/tool boundary, not every optional harness/integration. Schema validation does not prove factual truth. |
| OpenAI Agents SDK | Small agent/tool/handoff primitives, sessions, human involvement and tracing; model integrations include alternatives. [Official SDK](https://openai.github.io/openai-agents-python/) | Strong alternative when OpenAI is primary. Do not implement both SDKs in v1; review tracing/data egress settings before sending repository contents. |
| LangGraph | Explicit graph/state orchestration, persistence, streaming and human interrupts; LangChain is not required. [Overview](https://docs.langchain.com/oss/python/langgraph/overview) | Prefer if branching/resumption complexity grows. Two bounded initial apps do not justify a graph platform plus a second workflow engine. |
| Hugging Face smolagents | Compact Python library, CodeAgent and tool-calling agents, multiple models and sandbox integrations. [Official introduction](https://huggingface.co/docs/smolagents/index) | Good reference for simplicity; code execution expands the threat surface. WraithKit v1 should use typed allowlisted tools, not free-form generated Python. |
| Strands | Harness/SDK documentation covers model providers, persistent sessions, memory, structured outputs, approvals and MCP. [Official docs](https://strandsagents.com/latest/documentation/docs/) | Worth evaluating for AWS-oriented hosting. Not a reason to require cloud infrastructure for a local-first demo. No latency/resource benchmark performed. |
| Microsoft Agent Framework | Agents, workflows, model clients, context/memory and MCP integrations; capability availability differs by language/provider. [Official overview](https://learn.microsoft.com/en-us/agent-framework/overview/) | Suitable for an existing Microsoft stack; avoid importing its full platform solely for this prototype. |

Pydantic AI, LangGraph and OpenAI Agents SDK each currently publish MIT license files: [Pydantic](https://github.com/pydantic/pydantic-ai/blob/main/LICENSE), [LangGraph](https://github.com/langchain-ai/langgraph/blob/main/LICENSE), [OpenAI](https://github.com/openai/openai-agents-python/blob/main/LICENSE). Recheck pinned distributions and transitive dependencies; these facts do not license model outputs, datasets or external services.

## Decision
Proposed stack: Python 3.12+, `pydantic-ai` with only the chosen provider extra, SQLite from the standard library, and a minimal local web/CLI surface. Pick exact versions through a small compatibility spike, not unverified latest-version numbers in a spec. Use Pydantic AI's deferred-tool capability for pausing model interaction, while WraithKit owns the durable approval record and exact execution authorization. [Deferred tools](https://pydantic.dev/docs/ai/tools-toolsets/deferred-tools/)

Do not build a new generic agent orchestrator. The differentiator should be **two useful applications sharing inspectable permissions, evidence and restart-safe human control**, not 20 personas with different prompts. Memory, approvals and history are distinct stores/views, not synonyms for conversation history.

MCP is an optional adapter, not the trust boundary. The official guidance identifies confused-deputy, token-passthrough, SSRF and session-hijacking risks. Use destination/credential scoping, validate redirects, and never equate a connected server with owner authorization. [MCP security guidance](https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices)

## No benchmark claims
No packages installed, model requests made, GPU used or competing frameworks executed. This comparison evaluates documented fit. Implementation must verify restart behavior, provider schema differences and permission enforcement on the selected version. Existing products already cover much of this space; originality must come from polished evidence-grounded workflows and honest limits, not claiming the first memory/approval framework.
