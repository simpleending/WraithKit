# Output schema limits

These are proposed JSON Schema 2020-12 output contracts, not executable agents. The two example files are explicitly synthetic and make no findings or passed-test claims.

Application validation must additionally enforce: evidence IDs exist in the same run/workspace; hashes match acquired snapshots; path/ranges belong to that snapshot; line_end >= line_start; supported/inferred/conflicted claims have appropriate evidence; conflicted claims identify the disagreement; passed/failed tests have actual executor receipts; fixture evidence cannot be represented as live acquisition; patch paths and receipts resolve only inside the allowed artifact store. A string that resembles a hash is not evidence of acquisition. Schema conformance alone cannot establish any of these facts.

No JSON Schema validator was installed or run. The final audit checks JSON syntax, local document links and pinned upstream byte hashes only. The implementation task must validate schemas and examples with a standard pinned validator, then exercise these semantic constraints.
