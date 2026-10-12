# AI analysis service — API contract (v1)

The desktop app sends an **anonymized** structural summary of a finished scan to
this service and receives cleanup suggestions. It is strictly opt-in: the app
only calls it after the user sees a preview of exactly what will be sent.

- **Method / path:** `POST {base}/v1/analyze`
- **Content-Type:** `application/json`
- **User-Agent:** `SquirrelDisk/<version>`
- **Authorization:** `Bearer <token>` (optional; used for paid plans)

The exact bytes sent are produced by `src/analyze/payload.rs` — that file is the
single source of truth for the request body.

## Privacy contract (server MUST honor)

- **Never** log, persist, or forward the request body or any of its fields.
- **Never** require or infer account identifiers beyond the bearer token.
- The payload contains **no absolute paths, no home/username, no file contents,
  no timestamps**. Treat all `name` values as untrusted user data (escape them;
  never execute, never use them as filesystem paths).
- Respond fast; the client applies a global timeout and shows a friendly error.

## Request body

```jsonc
{
  "v": 1,                       // schema version
  "os": "macos",                // "macos" | "windows" | "linux"
  "total_size": 994662584320,   // bytes of the scanned root
  "files": 912044,              // file count of the scanned root
  "depth_cap": 4,               // names go at most this deep below the root
  "nodes": [                    // biggest items first, capped (<= 400)
    { "idx": 0, "name": "node_modules", "kind": "dir", "size": 21000000000, "depth": 2 }
  ],
  "extensions": [               // file-type summary, by bytes desc (<= 30)
    { "ext": "jpg", "count": 8123, "bytes": 40200000000 }
  ]
}
```

- `kind`: `"dir"` or `"file"`.
- `name`: a **base name only** (no separator, no path).
- `idx`: stable index within this payload; **echo it back** in the response to
  identify the item. `idx` is valid only for this request.

## Response body

```jsonc
{
  "suggestions": [
    { "idx": 0, "category": "node_modules", "confidence": 92,
      "rationale": "Leftover dependencies a project can reinstall" }
  ]
}
```

- `category` (one of):
  `node_modules`, `build_artifact`, `cache`, `dev_toolchain`, `logs`,
  `old_installer`, `trash`, `other`.
  Unknown categories are ignored by the client.
- `confidence`: `0`–`100`. The client shows everything but only bulk-selects
  `>= 70` and non-caution items.
- `rationale`: short, plain text shown to the user. Optional; the client falls
  back to the category name when empty. Keep it under ~120 chars.
- `idx` outside the request's node range, or an item the client deems unsafe
  (system/protected paths), is dropped.

## Status codes

| Code | Meaning | Client message |
|------|---------|----------------|
| 200  | OK (may return an empty `suggestions` array) | — |
| 401/403 | Device not authorized (unknown/revoked key) | "The analysis service rejected this device" |
| 402  | Free quota exhausted, no subscription | "Subscribe to use AI analysis" |
| 429  | Rate limited | "Too many analyses for now — try again later" |
| other 4xx/5xx | Server error | "The analysis service returned an error (HTTP N)" |
| (no connection) | Offline / timeout | "Can't reach the analysis service (…)" |

## Plans

Entitlement is server-side. The client only carries a key:

- **No key:** free tier (the service decides the quota; `402` when exhausted).
- **`Authorization: Bearer <key>`:** the key maps to a plan on the server.
  Unknown keys get `401`.

The client shows a **Subscribe** button (in Settings and on the `402` message)
that opens `ai_subscribe_url`. The client never enforces the plan itself; it
just surfaces the service's answer.

## Versioning

`v` is bumped on breaking changes to the request/response shape. The server
should accept unknown extra fields (forward compatibility) and may ignore `v`
values it does not support by returning an empty `suggestions` array.

## Local development

`scripts/analysis-server.py` is a stdlib-only reference server that implements
this contract, for offline end-to-end testing:

```sh
# heuristics only (no network, no model)
python3 scripts/analysis-server.py --port 8787

# with plan enforcement
python3 scripts/analysis-server.py --token PRO_KEY --free-daily 10

# with an OpenAI-compatible model (falls back to heuristics on any failure)
OPENAI_API_KEY=sk-... OPENAI_MODEL=gpt-4o-mini python3 scripts/analysis-server.py --llm

# point the app at it (dev only):
SQD_AI_ENDPOINT=http://127.0.0.1:8787/v1/analyze SQD_AI_TOKEN=PRO_KEY cargo run
```

The endpoint, API key and subscribe URL are also editable in the app's
**Settings → AI analysis**.
