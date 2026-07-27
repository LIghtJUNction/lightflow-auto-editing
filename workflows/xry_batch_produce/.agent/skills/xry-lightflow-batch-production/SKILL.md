---
name: xry-lightflow-batch-production
description: Use for the complete canonical XRY production chain for one frozen task subject through LightFlow.
version: 0.1.0
---

# LightFlow XRY Batch Production

Use `lightflow.xry_batch_produce` only after the Planner has selected one exact
frozen `批量剪辑/<完整分组名>/<批次名>` task and one exact `Sxx` subject. The
gateway owns the complete canonical production chain and returns the bound
worker context only in a verified canonical `PASS` response.

Do not supply legacy stage, package, path, command, or shell controls. Do not
call an XRY renderer, validator, cover tool, acceptance tool, or internal CLI
directly. If the gateway is unavailable, a request or response does not match
the bound task and subject, or canonical `PASS` is absent, stop and return the
blocker to the Planner; never synthesize a worker context or acceptance result.

```bash
lfw run lightflow.xry_batch_produce \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"' \
  --input subject='"S01"'
```

The returned `worker_context`, `production_report`, and `task_state_path` are
canonical evidence for that one run. They do not authorize cleanup, archive, or
publication; those require their own LightFlow control workflows and gates.

## HTTP Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.xry_batch_produce/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"task":"批量剪辑/皮卡严选 走全球/7.23批量","subject":"S01"}}'
```
