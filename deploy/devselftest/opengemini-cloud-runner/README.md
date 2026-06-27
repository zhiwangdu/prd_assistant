# openGemini Cloud Runner

This is a minimal Dockerized Python test runner for `logagent.dev_selftest.run_tests`
with dynamic `testParams`.

ToolHub does not create cloud instances for this flow. An external/internal skill creates the
openGemini-compatible instance, then calls `run_tests` with non-secret runtime parameters:

```json
{
  "runId": "devselftest_...",
  "testSuite": "cloud_opengemini_case",
  "testParams": {
    "caseName": "opengemini_rw_smoke",
    "instanceId": "local-demo",
    "endpoint": "http://127.0.0.1:8086"
  }
}
```

Build the sample image:

```bash
docker build -t localtoolhub/opengemini-selftest:dev deploy/devselftest/opengemini-cloud-runner
```

Configure the test suite image as `localtoolhub/opengemini-selftest:dev` for local validation.
In the intranet deployment, replace only the image with the approved internal image that carries
internal SDKs or credential lookup logic.

The runner writes artifacts to `SELFTEST_ARTIFACTS_DIR` when set, otherwise to the mounted
container path `/workspace/artifacts`. ToolHub's `DEVSELFTEST_ARTIFACTS_DIR` is a host path and is
only used as a last-resort fallback outside the normal Docker profile.

`testParams` must never contain credentials. ToolHub passes them to Docker with `--env KEY=VALUE`,
so values are visible in the host-side `docker run` argv while the container starts.
