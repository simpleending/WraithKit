# Owner direction update: Rust preferred

After the initial research handoff, Noble chose Rust as WraithKit's top stack choice for memory safety, as relayed by the root coordinator. This supersedes the initial Python-first recommendation as the preferred direction, not as a claim that implementation or compatibility testing has occurred.

PB work remains the immediate priority. The follow-up is to evaluate a small Rust core versus a Python-first implementation, including model/tool adapters, durable approvals and memory, developer effort and interoperability. Memory safety alone does not establish authorization, prompt-injection resistance, filesystem sandboxing, factual accuracy or crash-safe external actions. A possible high-design landing page is later work, not permission to deploy.

The original handoff and MANIFEST.json are preserved as a dated research snapshot. This additive direction note is not included in that earlier manifest. No code, package or runtime changes accompany this note.
