## Context

`wfb-link` now ships a local Android AAR and the Pixel/RTL8812AU bench can pass
a managed RF smoke. The remaining integration work is above RF: a product app
needs a normal Android sample, lifecycle-safe Java API, named stream
configuration, and repeatable build validation. The current SDK still exposes
one blocking method and a smoke-shaped managed stream config.

## Goals / Non-Goals

**Goals:**

- Make the Android SDK easy to consume from a standard Android project layout.
- Provide Java API shape that can be used from a foreground service or worker:
  start, stop, status, callback/result, and cleanup.
- Let product code describe named raw WFB streams without embedding product
  meaning in the native radio layer.
- Validate Android SDK packaging in local scripts and GitHub Actions.
- Keep the smoke harness useful for hardware regression.

**Non-Goals:**

- Publishing the AAR to Maven Central or an internal registry.
- Implementing a full app-owned foreground service in the SDK.
- Replacing WFB-NG helper processes with a native Rust codec.
- Solving long-range RF tuning in this change.

## Decisions

### Keep AAR Local, Add Gradle Consumer Sample

The repo will continue producing `target/android-sdk-aar/wfb-link-android-sdk-debug.aar`
with deterministic SDK/NDK scripts. A sample Android project will consume that
AAR from a local `libs/` path and demonstrate the real app contract.

Alternative considered: introduce a full top-level Gradle build and make the
SDK an Android library module. That is better for later publishing, but it is a
larger migration than needed for product adoption this week.

### Add Lifecycle Facade Above Blocking Native Call

The native runtime remains a blocking call for now. The SDK adds a Java
`WfbManagedStreamsSession` wrapper that runs the blocking call on an
`ExecutorService`, exposes status snapshots, supports cooperative stop request
state, and reports completion through callback/future-style APIs.

Alternative considered: rewrite native runtime execution to be nonblocking and
interruptible immediately. That is the right long-term shape, but a Java
lifecycle wrapper gives product apps a safe integration surface without
destabilizing the proven native RF path.

### Model Named Streams In Java Before Native Multiplexing

The SDK will introduce `WfbManagedStream` definitions with name, direction,
radio port, local UDP port, link ID, payload kind, criticality, and TX profile.
The current native path can map the first configured TX/RX stream pair into the
existing smoke-proven ports while rejecting unsupported shapes explicitly. This
keeps the product config shape aligned with `wfb-link` while making remaining
native multiplexing work visible instead of implicit.

Alternative considered: wait to add stream models until native Android supports
N stream pairs. That would keep the API smaller but force downstream apps to
rewrite config later.

### CI Uses Available Android Tooling Opportunistically

GitHub Actions should always run Rust workspace tests. Android AAR/consumer
checks should run when SDK/NDK tooling is installed by the workflow. Hardware RF
smokes remain manual bench gates.

## Risks / Trade-offs

- Java stop requests cannot interrupt every native USB operation yet. Mitigate
  by documenting that stop is cooperative and by keeping bounded durations in
  sample code.
- Named stream config may outpace native Android multiplexing. Mitigate with
  explicit validation errors for unsupported multi-stream shapes.
- Gradle sample can drift from direct scripts. Mitigate by compiling it in the
  Android SDK validation script and CI.

## Migration Plan

1. Add Java session/status/callback and named stream config classes.
2. Update smoke and consumer samples to use the lifecycle facade.
3. Add Gradle consumer sample and script-level validation.
4. Add GitHub Actions coverage for host and Android SDK builds.
5. Update docs and mark unsupported Android multi-stream shapes explicitly.
