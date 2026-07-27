---
name: lightflow-xry-privacy-redaction
description: This skill guides agents through canonical preview and explicitly confirmed XRY privacy redaction for one frozen task subject.
version: 0.1.0
---

# LightFlow XRY Privacy Redaction

Use `lightflow.xry_privacy_redaction` only for one exact frozen
`批量剪辑/<group>/<batch>` task and one exact `Sxx` subject. Preserve both
bindings for every invocation.

First request a preview with `apply=false`. Use `apply=true` only after the
user explicitly confirms the exact `approval_plan_sha256` from that prior
preview and supplies an opaque `confirmation_receipt_ref` for that
confirmation.

Never supply direct SSH, XRY command, path, or other bypass controls. Never
fabricate a canonical `PASS`, redaction outcome, or result. Stop and report the
blocker on any canonical gateway failure.

```bash
lfw run lightflow.xry_privacy_redaction \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"' \
  --input subject='"S01"' \
  --input apply=false
```

```bash
# Only after explicit user confirmation of this exact prior preview plan SHA-256.
lfw run lightflow.xry_privacy_redaction \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"' \
  --input subject='"S01"' \
  --input apply=true \
  --input plan_sha256='"<exact-prior-plan-sha256>"' \
  --input confirmation_receipt_ref='"opaque:<confirmation-receipt-ref>"'
```

## API Usage

This endpoint requires `lfw serve` and retains the same explicit-confirmation
rule for `apply=true`: use the exact prior preview plan SHA-256 and an opaque
confirmation receipt reference only after the user confirms it.

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.xry_privacy_redaction/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"task":"批量剪辑/皮卡严选 走全球/7.23批量","subject":"S01","apply":false}}'
```
