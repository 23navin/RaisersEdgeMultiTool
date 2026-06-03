// re-calls.ts
//
// Central catalog of parameterized RE NXT (Blackbaud SKY) API calls — the TS
// half of the hybrid API-call library. This catalog drives the UI (parameter
// forms, labels, validation) and is shared by both Report profiles and the Data
// Requests tab. The actual HTTP/auth/polling lives in the Rust registry
// (src-tauri/src/re_calls.rs, future) and is invoked via a generic
// `run_re_call(callId, params)` command.
//
// A bundle may also ship its own call definitions (the per-bundle half of the
// hybrid model); those are merged over this catalog at load time.
//
// SHELL ONLY: definitions/signatures, no execution logic yet. See
// REPORT_PROFILES.md for the full contract.

// query_execute hides the SKY async pattern (execute → poll job → page results)
// so authors never deal with job polling. rest_get / rest_post are thin wrappers
// over arbitrary SKY endpoints.
export type ReCallKind = "query_execute" | "rest_get" | "rest_post";

export type ReCallParam = {
  name: string;            // dotted path addresses nested request fields, e.g. "filters.gift_date.from"
  type: string;            // "string" | "number" | "date" | "id_list" ...
  required: boolean;
  description?: string;
};

export type ReCallDefinition = {
  id: string;              // referenced from a profile's `queries[].ref` / `actions[].ref`
  name: string;
  description?: string;
  kind: ReCallKind;
  params: ReCallParam[];
  // Hint describing the call's result for the transform/UI layer. For
  // query_execute the rows are written as JSON for DuckDB's read_json_auto.
  result?: { format: "json"; rowsPath?: string };
};

// The two seed entries. Expand as more RE operations are abstracted.
export const RE_CALLS: Record<string, ReCallDefinition> = {
  "re.query.execute": {
    id: "re.query.execute",
    name: "Execute RE Query",
    description:
      "Run a saved or ad-hoc RE query and return its rows. Hides the async " +
      "execute → poll → page job flow.",
    kind: "query_execute",
    params: [
      { name: "query_id", type: "number", required: false, description: "Saved RE query id (omit when supplying an inline definition)." },
      { name: "definition", type: "string", required: false, description: "Inline ad-hoc query definition (alternative to query_id)." },
    ],
    result: { format: "json" },
  },
  "re.query.create": {
    id: "re.query.create",
    name: "Create RE Query",
    description:
      "Create/save a query in the RE CRM from a list of record ids so users " +
      "can reference it inside RE's UI.",
    kind: "rest_post",
    params: [
      { name: "name", type: "string", required: true, description: "Display name for the created query." },
      { name: "ids", type: "id_list", required: true, description: "Record ids the query should contain." },
    ],
  },
};

// Resolve a call by id from the central catalog.
export function getReCall(id: string): ReCallDefinition | undefined {
  return RE_CALLS[id];
}
