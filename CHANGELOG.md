# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.3.0] - 2026-03-09

### Added

- Full upstream root-cause traversal output with dependency chains to first broken ancestor.
- Blast-radius impact scoring and ranking (`rank`, `impact_score`) across `Pod`, `Service`, `Deployment`, and `Ingress` impact sets.
- SARIF enrichment with impact metadata (`impact_score`, `impact_rank`).
- Network reachability analyzer with graph-backed RCA for policy-blocked traffic paths.
- NetworkPolicy simulation coverage for:
  - `podSelector` + `namespaceSelector` peer matching (including cross-namespace scenarios)
  - selector match expressions (`In`, `NotIn`, `Exists`, `DoesNotExist`)
  - named ports and numeric/range port matching
  - `ipBlock` / `except`-aware external egress reasoning
- Namespace collector and normalized namespace labels in `AnalysisContext`.
- Normalized pod container ports in `PodState` for named-port resolution.
- New analyzer integration tests for network reachability scenarios.

### Changed

- Graph builder now emits richer blocked-path edges for:
  - `Ingress -> NetworkPolicy` (external ingress path blocks)
  - `Service -> NetworkPolicy` (internal client path blocks)
  - `Pod -> NetworkPolicy` (egress peer/port mismatch blocks)
- Default analyzer registry now runs the new network reachability RCA flow.
- CLI text/JSON/SARIF golden fixtures updated to reflect new reachability traces and impact ranking output.
- README updated for new reachability behavior, ranking output, limitations, and roadmap.

## [0.1.0] - 2026-03-08

### Added

- Initial `kroot` CLI:
  - `kroot diagnose cluster`
  - `kroot diagnose pod <name>`
- Namespace controls: `-n/--namespace`, `-A/--all-namespaces`
- Output formats: `text`, `json`, `sarif`
- Offline analysis via `--context-file`
- Analyzer engine, analyzer trait, and registry
- Built-in analyzers (CrashLoopBackOff, ImagePullBackOff, OOMKilled, Unschedulable, Missing Secret/ConfigMap, Service selector mismatch, Node NotReady, NetworkPolicy-related)
- Dependency graph layer and key relations (`Deployment -> ReplicaSet -> Pod`, `Ingress -> Service`, `Service -> Pod`, `Pod -> Secret/ConfigMap/PVC/Node`, `PVC -> PV`)
- Confidence scoring and diagnosis ranking

### Changed

- README roadmap updated to separate completed milestones vs next milestones

### CI / Release

- GitHub Actions CI workflow for build/test
- Tagged release workflow for publishing binaries
