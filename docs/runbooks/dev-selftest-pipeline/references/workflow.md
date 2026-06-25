# Dev Self-Test Workflow And Result Shapes

The converged dev_selftest workflow implements a Docker-based self-test loop. All
commands, binaries, compose files, repositories, and refs come from the `dev_selftest`
configuration allowlist. Tool parameters only select profile ids and carry `runId`.

## Config Shape

```yaml
remote_execution:
  commands:
    smoke:
      enabled: true
      argv: ["sh", "/tests/smoke.sh"]
      timeout_seconds: 180

dev_selftest:
  enabled: true
  git:
    enabled: true
    binary: "/usr/bin/git"
    repos:
      - { url: "https://example/project.git", refs: ["main", "release/*"] }
  builds:
    project:
      command: ["/opt/localtoolhub/scripts/build-project.sh"]
      working_dir: ""
      artifact_globs: ["build/bin/project"]
  docker:
    binary: "/usr/bin/docker"
    clusters:
      local_cluster:
        compose_file: "/opt/localtoolhub/devselftest/docker-compose.yml"
        exposed_port: 8086
        health_check:
          cmd: ["curl", "-sf", "http://127.0.0.1:8086/health"]
          timeout_seconds: 180
  test_suites:
    smoke:
      command: smoke
      timeout_seconds: 180
      docker:
        image: "alpine:3.20"
        network: "host"
        volumes:
          - "/opt/localtoolhub/devselftest/tests:/tests:ro"
```

`remote_execution.commands` is only a command-template table for dev_selftest suites. It
does not enable SSH, SCP, managed executor records, or arbitrary remote command execution.

## Step Results

### sync_workspace

```json
{
  "runId": "devselftest_...",
  "sourceRef": "git:https://example/project.git@main",
  "status": "OK",
  "durationMs": 123
}
```

Source options are `uploadId`, `gitRepo` + `gitRef`, or an empty stub source.

### build

```json
{
  "runId": "devselftest_...",
  "buildProfile": "project",
  "status": "OK",
  "exitCode": 0,
  "artifacts": ["artifacts/project"],
  "durationMs": 3456
}
```

### deploy

```json
{
  "runId": "devselftest_...",
  "profile": "local_cluster",
  "projectName": "devselftest_devselftest_..._local_cluster",
  "status": "OK",
  "exitCode": 0,
  "deployTarget": {
    "kind": "docker",
    "cluster": "local_cluster",
    "exposed_port": 8086
  }
}
```

### run_tests

```json
{
  "runId": "devselftest_...",
  "testSuite": "smoke",
  "status": "OK",
  "exitCode": 0,
  "executor": {
    "kind": "docker",
    "image": "alpine:3.20",
    "network": "host"
  },
  "stdoutPath": "logs/tests.stdout.txt",
  "stderrPath": "logs/tests.stderr.txt"
}
```

For suites without `docker`, the result uses the local stub runner with configured `argv`.

### report

```json
{
  "runId": "devselftest_...",
  "status": "SUCCEEDED",
  "reportPath": "report.md",
  "failedSteps": [],
  "steps": []
}
```

## progress.json

```json
{
  "schemaVersion": 1,
  "runId": "devselftest_...",
  "steps": [
    { "step": "sync_workspace", "status": "OK", "durationMs": 12, "error": null, "evidenceRefs": ["source/"] },
    { "step": "build", "status": "OK", "durationMs": 345, "evidenceRefs": ["artifacts/project"] },
    { "step": "deploy", "status": "OK", "evidenceRefs": ["logs/deploy.stdout.txt", "logs/deploy.stderr.txt"] },
    { "step": "run_tests", "status": "OK", "evidenceRefs": ["logs/tests.stdout.txt", "logs/tests.stderr.txt"] },
    { "step": "report", "status": "OK", "evidenceRefs": ["report.md", "report.json"] }
  ]
}
```

Re-running a step replaces its previous entry. A `FAILED` step marks the dev_selftest run
failed; `report` remains callable and lists `failedSteps`.
