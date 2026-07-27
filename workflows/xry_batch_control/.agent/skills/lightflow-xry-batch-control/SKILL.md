---
name: lightflow-xry-batch-control
description: Use for canonical, bound XRY progress, freeze, cleanup, or archive control through LightFlow.
version: 0.1.0
---

# LightFlow XRY Batch Control

Use `lightflow.xry_batch_control` only for one exact frozen
`批量剪辑/<完整分组名>/<批次名>` task and one exact `Sxx` subject. It is the
only public agent entrypoint for `progress`, `freeze`, `cleanup`, and `archive`.
The workflow has no global status mode and no arbitrary path or shell input.

The public workflow uses a locked, framed gateway protocol. Never replace it
with direct SSH, an XRY command, a renderer, a validator, or a locally inferred
task state. If the gateway does not return its verified canonical `PASS`, stop
the next stage and report the failure without guessing a fallback.

For `cleanup` and `archive`, first run with `apply=false`. Read the exact
canonical plan SHA-256 from the returned canonical report, present that exact
hash to the user, and only run `apply=true` after explicit confirmation of the
same hash. `apply=true` without the exact plan hash is rejected. Do not use this
workflow for production, packaging, publishing, deletion outside its plan, or
reference-tree edits.

```bash
lfw run lightflow.xry_batch_control \
  --input action='"progress"' \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"' \
  --input subject='"S01"'
```

```bash
# Only after the user confirms this exact plan SHA-256 from a prior dry run.
lfw run lightflow.xry_batch_control \
  --input action='"archive"' \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"' \
  --input subject='"S01"' \
  --input apply=true \
  --input plan_sha256='"<confirmed-plan-sha256>"'
```

## HTTP Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.xry_batch_control/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"action":"progress","task":"批量剪辑/皮卡严选 走全球/7.23批量","subject":"S01"}}'
```
