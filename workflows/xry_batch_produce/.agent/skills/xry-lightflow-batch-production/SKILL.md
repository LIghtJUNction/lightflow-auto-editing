---
name: xry-lightflow-batch-production
description: Rerender rejected XRY batch subjects through the deployed Rust LightFlow worker.
version: 0.2.0
---

# XRY batch production through LightFlow

Use `lightflow.xry_batch_produce` for one existing subject in a task under
`/srv/2.预处理/批量剪辑/`. Its published `runner.v1` binary invokes the
deployed Rust worker on `ssh xry`. The subject must already be marked
`REJECTED`; this is intentional so a stale or failed output cannot be silently
reused. `cover-spec.json` must explicitly provide `headline_zh` containing CJK
characters for ZE and `headline_ru` containing Cyrillic characters for RE.
The worker does not infer either headline from IDs, subtitles, a generic
`headline`, or the EDL; invalid cover text leaves both existing cover files
untouched.

The workflow rerenders the frozen EDL and captions without a shell, creates ZE
(Chinese/English) and RE (Russian/English), and creates two account-specific
vehicle-frame covers. It does not package a delivery. Run the separate
`lightflow.xry_batch_control` action `audit`, then `package`; the worker rejects
both packaging and delivery whenever the current quality gate fails.

`commit_package`, `from_stage`, and `to_stage` are retained only for contract
compatibility and are rejected by the Rust worker.

## CLI Usage

```bash
lfw run lightflow.xry_batch_produce \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"' \
  --input subject='"S01"'
```

## API Usage

Start `lfw serve`, then call the shared HTTP
workflow contract:

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.xry_batch_produce/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"task":"批量剪辑/皮卡严选 走全球/7.23批量","subject":"S01"}}'
```
