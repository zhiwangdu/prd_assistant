---
name: openGemini Diagnosis
description: Diagnose openGemini and InfluxDB-compatible cluster, shard, PT, metadata, and log issues.
---

Use this skill when the task involves openGemini or InfluxDB-compatible deployments.

Focus the first pass on current task evidence:
- Bind log time windows to the user's question before interpreting topology.
- Compare Metadata adapter facts for Instance, Node, Cluster, Database, PT, Shard, and Index state.
- Treat System Context and this skill as background only; final root cause evidence must come from task evidence such as session text, grep, tool findings, or case context.
- Prefer checking PT owner, Shard owner, Index owner, node status, and connection fields before proposing storage or query-layer causes.

When topology or terminology details are needed, read the declared references through `logagent.get_skill_reference`.
