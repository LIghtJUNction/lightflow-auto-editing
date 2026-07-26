---
name: lightflow-video-work-mcp
description: This skill should be used when the user asks to "configure Video Work API MCP", "connect LightFlow to Video Work API", "secure the Video Work MCP endpoint", or "troubleshoot LIGHTFLOW_VIDEO_WORK_API_URL".
version: 0.3.0
---

# LightFlow Video Work API MCP client

Configure the shared Rust-native client through the runtime-only
`LIGHTFLOW_VIDEO_WORK_API_URL` and either `LIGHTFLOW_VIDEO_WORK_API_TOKEN` or
`LIGHTFLOW_VIDEO_WORK_API_TOKEN_FILE`. Supply the API base URL, not the `/mcp`
suffix; the client addresses the MCP endpoint itself. Prefer a non-empty token
environment value when present. Otherwise supply a path to a regular UTF-8
token file no larger than 4 KiB; its contents are trimmed and must remain
non-empty.

Require an HTTP or HTTPS URL with a host. Require HTTPS for every non-loopback
endpoint. Permit HTTP only for `127.0.0.1`, `localhost`, or `::1`. Never put
the URL, token, token file path, or token file contents in workflow inputs,
artifacts, source files, prompts, logs, or command output.

Route only the supported Video Work workflow actions through this client. The
client unwraps `result.structuredContent` from a JSON-RPC `tools/call` reply;
that value must be an object. Treat missing runtime configuration, oversized
responses, malformed JSON, missing result layers, redirects, or MCP errors as
failures; do not retry by exposing credentials or by bypassing the authenticated
MCP endpoint.
