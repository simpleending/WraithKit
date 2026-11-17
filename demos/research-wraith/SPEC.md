# Research Wraith: proposed second application

Audience: a builder choosing between technologies who needs a traceable answer with conflicts and uncertainty, not fluent unsupported summaries.

Input: research question plus permitted public sources or owner-provided document set. First run uses synthetic/locally stored fixtures; network mode is separately enabled and visibly labeled.

Working demo story:
1. Ask a concrete comparison question. Fetch/read bounded primary-source documents and record URL/title/time/hash and text locator.
2. Build a claim/evidence matrix: supported, inference, conflict or unknown. Show a changed deadline in two dated fixtures; prefer the authoritative current source but retain contradiction.
3. Produce a concise cited brief and a source list. A blocked/dead link stays a visible gap rather than a fabricated quote.
4. Propose remembering a preference. User declines: it is not saved. User approves a different preference: store provenance/scope/expiry. Restart and retrieve it only in the same workspace.
5. Inject a page telling the agent to reveal memory or approve a tool. The policy dispatcher rejects any scope expansion; history records the refusal without leaking secrets.

Tools: search_public (later connector), fetch_public_https, read_document, evidence_register, draft_brief, propose_memory. No authenticated browsing, outbound email, automatic posting or purchase.

Typed output: answer, claims[{text,status,evidence_ids}], source_ids, unresolved_questions and suggested_next_checks. A source ID must resolve to an actually acquired record. Snippets do not substitute for reading the cited content. Avoid excessive quoting and preserve source terms.

Pass criteria: every material claim has usable evidence or an explicit unsupported label; conflicts survive summarization; no private/metadata IP access even through redirects; no automatic memory promotion; network absence yields a useful fixture-only demonstration rather than a fake live badge.

Status: specification only. No functioning application, search connector or model evaluation exists yet.
