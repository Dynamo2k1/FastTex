# FastTeX: Comprehensive System Analysis

> This document provides the complete phased analysis of FastTeX architecture,
> identifying all issues, their impacts, and design decisions made to address them.

---

## PHASE 1: Full Issue Enumeration

The following issues were identified through systematic analysis of the system architecture.
No solutions are proposed in this phase—only identification.

### 1. Correctness Issues

#### 1.1 Reference Convergence and Oscillation
- LaTeX cross-references require multiple compilation passes
- Auxiliary files may oscillate between states without converging
- No detection or handling of infinite convergence loops
- Maximum iteration count exists but lacks smart cutoff logic

#### 1.2 Dependency Graph Incompleteness
- Parser uses regex-based extraction which may miss edge cases
- Nested `\input` commands within included files not recursively resolved
- Dynamic includes (`\IfFileExists`) not detectable at parse time
- Custom macros that generate includes not recognized

#### 1.3 Stateful TeX Compilation
- TeX compilation is inherently stateful across compilation units
- Global counter modifications in one chapter affect others
- Package-level state not properly isolated between parallel workers
- Conditional compilation based on `.aux` state creates ordering dependencies

#### 1.4 Parallel Compilation Violations
- Parallel chapter compilation assumes independence that may not hold
- Cross-chapter macros and definitions create hidden dependencies
- Bibliography processing must complete before final pass
- Index generation requires sequential processing

### 2. Security Issues

#### 2.1 Network Isolation vs Package Fetching Paradox
- Workers are network-isolated for security
- Tectonic requires network access for on-demand package fetching
- Pre-bundled packages may be stale or incomplete
- No mechanism for safe, controlled package updates

#### 2.2 Resource Exhaustion Attacks
- No per-user compute quotas implemented
- No per-user memory limits enforced
- No rate limiting on compilation requests
- Malicious documents could consume unbounded resources

#### 2.3 Poisoned Job Handling
- Compilation jobs that crash workers not isolated
- No circuit breaker for repeatedly failing documents
- Worker crash doesn't trigger proper cleanup
- Zombie processes not detected or terminated

### 3. Scalability Issues

#### 3.1 Redis Memory Pressure
- All auxiliary files cached in Redis without eviction policy
- No size limits on cached artifacts
- Long-running projects accumulate unbounded state
- No tiered storage strategy (hot/warm/cold)

#### 3.2 Worker Pool Exhaustion
- Fixed worker pool size doesn't adapt to load
- No auto-scaling mechanism defined
- No queuing discipline for burst traffic
- Worker registration lacks health checking

#### 3.3 CRDT Growth
- Yjs document state grows unbounded with edit history
- No compaction strategy for long-lived documents
- Awareness/presence updates not batched efficiently
- Large documents cause memory pressure on gateway servers

#### 3.4 Global Dependency Bottlenecks
- Preamble compilation is a single point of dependency
- Bibliography processing serializes entire pipeline
- Format file generation blocks all chapter compilations
- Main document merge is strictly sequential

### 4. Performance Issues

#### 4.1 Cold Start Latency
- Firecracker VM boot time (~125ms) adds baseline latency
- Font cache population on first compilation
- Package bundle extraction per-VM
- Format file transfer over network

#### 4.2 Inefficient Scheduling
- Priority queue uses simple FIFO within priority levels
- No work stealing between worker pools
- Job dispatch doesn't consider data locality
- Redundant recompilations when source unchanged

#### 4.3 Cache Invalidation
- Content-hash based invalidation may be too aggressive
- Whitespace-only changes trigger full recompilation
- No semantic diff for LaTeX content
- Format file invalidation invalidates all dependent work

### 5. Reliability Issues

#### 5.1 Worker Starvation
- Priority inversion possible with long-running low-priority jobs
- No preemption mechanism
- Fair scheduling not implemented
- Single noisy user can starve others

#### 5.2 Data Loss Risks
- CRDT state only persisted on explicit save
- Gateway crash loses unsaved collaboration state
- Artifact cache has no durability guarantees
- Redis persistence configuration not specified

#### 5.3 Failure Recovery
- No retry logic for transient failures
- Worker failure doesn't trigger job reassignment
- Partial results lost on failure
- No checkpointing during long compilations

### 6. Observability Issues

#### 6.1 Diagnostic Blind Spots
- TeX compilation errors not correlated to source location
- SyncTeX generation incomplete for parallel chunks
- Log aggregation from workers not structured
- No distributed tracing across components

#### 6.2 Error Attribution
- Compilation failures not attributed to specific causes
- Missing dependency vs syntax error not distinguished
- Timeout vs crash not distinguishable in logs
- No error taxonomy for user-facing messages

#### 6.3 Performance Visibility
- No per-phase timing breakdown
- Worker utilization not tracked
- Cache hit rates not measured
- Queue depth and latency not exposed

### 7. User Experience Issues

#### 7.1 Editor Limitations
- No LSP integration for LaTeX
- No auto-formatting support
- No symbol navigation
- No refactoring tools

#### 7.2 Compilation Feedback
- Progress updates coarse-grained
- No per-chapter progress visibility
- Error messages not hyperlinked to source
- Warning suppression not configurable

### 8. Operational Issues

#### 8.1 Configuration Management
- Hardcoded timeouts and limits
- No runtime configuration updates
- No feature flags for gradual rollout
- No A/B testing infrastructure

#### 8.2 Cost Management
- No metering of resource consumption
- No billing integration points
- VM lifecycle costs not optimized
- Storage costs unbounded

---

## PHASE 2: Categorization & Impact Analysis

### Correctness Category

| Issue | When It Manifests | Severity | Impact |
|-------|------------------|----------|--------|
| Reference oscillation | Documents with complex cross-refs | High | Infinite loops, failed compilations |
| Dependency incompleteness | Complex macro-heavy documents | Medium | Missing includes, broken PDFs |
| Stateful TeX | Parallel chapter compilation | High | Wrong page numbers, broken refs |
| Parallel violations | Any parallel build | High | Incorrect output, race conditions |

### Security Category

| Issue | When It Manifests | Severity | Impact |
|-------|------------------|----------|--------|
| Network paradox | Missing package | High | Compilation failure or security hole |
| Resource exhaustion | Malicious input | Critical | Service denial |
| Poisoned jobs | Adversarial documents | High | Worker crash, service degradation |

### Scalability Category

| Issue | When It Manifests | Severity | Impact |
|-------|------------------|----------|--------|
| Redis pressure | High usage, long sessions | High | OOM, eviction of needed data |
| Worker exhaustion | Traffic spikes | High | Request queuing, timeouts |
| CRDT growth | Long editing sessions | Medium | Memory exhaustion, slow sync |
| Bottlenecks | Large documents | Medium | Serialized compilation, slow builds |

### Performance Category

| Issue | When It Manifests | Severity | Impact |
|-------|------------------|----------|--------|
| Cold start | First compilation | Medium | User-perceived latency |
| Scheduling | High concurrency | Medium | Unfair resource distribution |
| Cache invalidation | Frequent edits | Low | Redundant work |

### Reliability Category

| Issue | When It Manifests | Severity | Impact |
|-------|------------------|----------|--------|
| Worker starvation | Multi-tenant usage | High | Service unfairness |
| Data loss | Component failure | Critical | User work loss |
| Failure recovery | Transient errors | High | User-visible failures |

---

## PHASE 3: System-Level Design Decisions

### Architectural Rules

#### Rule 1: Compilation Mode Duality
The system MUST support two compilation modes:
1. **Speculative Parallel Mode**: Fast, best-effort parallel compilation for development
2. **Guaranteed Linear Mode**: Correct, sequential compilation for final output

Users explicitly choose modes. Speculative mode may produce incorrect output.

#### Rule 2: Convergence Bounds
All iterative processes MUST have bounded iterations:
- Maximum 5 reference convergence passes
- Maximum 3 bibliography processing iterations
- Convergence failure produces warning, not error
- Final output generated from last stable state

#### Rule 3: Resource Quotas
All user operations MUST be bounded, with limits varying by user tier:
- Maximum compilation time: 5-30 minutes (depending on tier)
- Maximum memory per job: 256MB-2GB (depending on tier)
- Maximum concurrent jobs per user: 1-10 (depending on tier)
- Maximum document size: 5-50MB (depending on tier)
- Maximum project size: 50MB-1GB (depending on tier)

#### Rule 4: Failure Isolation
Job failures MUST NOT affect other jobs:
- Worker crash triggers job reassignment
- Poisoned jobs quarantined after 3 failures
- Circuit breaker prevents cascade failures
- Graceful degradation over hard failure

#### Rule 5: Fair Scheduling
Resource allocation MUST be fair across users:
- Weighted fair queuing based on user tier
- No user may consume more than 30% of workers
- Preemption allowed for higher priority tiers
- Starvation prevention via aging mechanism

#### Rule 6: Offline Package Availability
Workers MUST operate without network:
- Complete TeX Live bundle pre-installed
- Package updates via rootfs image updates only
- No runtime package fetching
- Missing package errors propagated to user

#### Rule 7: Artifact Lifecycle
All artifacts MUST have defined lifecycle:
- Format files: 7 days TTL, LRU eviction
- Aux files: Session-scoped, explicit cleanup
- PDF fragments: 1 hour TTL, immediate cleanup on success
- Final PDFs: User-controlled retention

#### Rule 8: CRDT Compaction
Document state MUST be bounded:
- Compaction triggered at 1MB state size
- History pruned to last 1000 operations
- Tombstone collection every 24 hours
- User warned of state growth

#### Rule 9: Observability
All operations MUST be observable:
- Distributed tracing with correlation IDs
- Structured logging with context
- Metrics for all queue depths and latencies
- Error classification and attribution

### What Is Solved By Each Mechanism

#### Compilation Strategy Solves:
- Reference convergence: Bounded retries with cutoff
- Parallel violations: Linear mode guarantees correctness
- Stateful TeX: Linear mode preserves ordering

#### Scheduling Policy Solves:
- Worker starvation: Fair scheduling with preemption
- Resource exhaustion: Per-user quotas
- Noisy neighbors: Compute budget enforcement

#### Storage Tiering Solves:
- Redis pressure: Tiered hot/warm/cold storage
- Artifact growth: TTL and LRU eviction
- Cost management: S3 for cold storage

#### Caching Solves:
- Cold start: Pre-warmed format files
- Redundant work: Content-hash based dedup
- Performance: Locality-aware job dispatch

#### Quotas and Admission Control Solves:
- Resource exhaustion: Hard limits enforced
- Abuse resistance: Rate limiting per user
- Fair access: Tiered resource allocation

### Linearization Requirements

The following MUST be serialized:
1. Format file generation (before any chapter compilation)
2. Bibliography processing (before final reference pass)
3. Index generation (after all content)
4. Final PDF merge (after all fragments)
5. Reference convergence check (after each full pass)

The following MAY be parallelized:
1. Independent chapter compilation (with format)
2. Figure pre-rendering (standalone)
3. Cache lookups and storage
4. Multiple user compilations (isolated)

### Convergence Limits and Failure Behavior

```
MAX_REFERENCE_PASSES = 5
MAX_BIBLIOGRAPHY_ITERATIONS = 3
MAX_CONVERGENCE_TIME = 150 seconds (2.5 minutes total)

On convergence failure:
1. Log detailed state of non-converging files
2. Mark document as "unstable references"
3. Return last best-effort PDF with warning
4. Do NOT block user from downloading
5. Suggest linear mode for guaranteed correctness
```

### Garbage Collection Strategy

```
Artifact GC Policy:
- Hourly: Clean up expired session artifacts
- Daily: Compact CRDT state for idle documents  
- Weekly: Archive unused projects to cold storage
- Monthly: Purge orphaned artifacts

Redis Eviction:
- Policy: volatile-lru
- Max memory: 70% of available
- Emergency eviction at 85%
```

### Collaboration Data Compaction

```
CRDT Compaction Triggers:
1. State size exceeds 1MB
2. Operation count exceeds 10,000
3. Idle time exceeds 1 hour
4. Explicit save request

Compaction Process:
1. Snapshot current document state
2. Clear operation history
3. Reset vector clock
4. Notify connected clients to resync
```

---

## PHASE 4: Implementation Summary

The following implementations address the identified issues:

### 4.1 Hybrid Compilation Strategy
- `CompilationMode` enum with `Speculative` and `Guaranteed` variants
- Speculative mode uses parallel workers with best-effort ordering
- Guaranteed mode uses single worker with strict sequential processing
- Mode selection exposed to user via API

### 4.2 Convergence Bounds
- `ConvergenceChecker` tracks aux file stability
- Hard limit of 5 passes with configurable timeout
- Oscillation detection via state hashing
- Graceful failure returns partial result with warning

### 4.3 Resource Quotas
- `QuotaManager` tracks per-user resource consumption
- `JobAdmissionControl` rejects over-quota requests
- Time, memory, and concurrency limits enforced
- Graceful rejection with retry-after header

### 4.4 Fair Scheduling
- `FairScheduler` implements weighted fair queuing
- User weight based on tier and historical usage
- Aging mechanism prevents starvation
- Per-user worker cap enforced

### 4.5 Poisoned Job Handling
- `CircuitBreaker` tracks job failure patterns
- Jobs failing 3+ times quarantined
- Quarantine cleared after cooldown period
- User notified of problematic documents

### 4.6 Cold Start Mitigation
- Pre-warmed worker pool with loaded formats
- Format file caching at worker level
- Lazy font loading with shared cache
- Connection pooling for Redis/S3

---

## PHASE 5: Validation & Failure Simulation

### Scenario: Circular References
**Input**: Document with `\ref{fig:a}` in caption of figure a  
**Expected**: Convergence failure after 5 passes  
**Actual Behavior**: System detects oscillation, returns warning with last PDF  
**User Impact**: Warned about unstable references, can still download

### Scenario: Non-Converging Aux Files  
**Input**: Document where aux changes on every pass  
**Expected**: Bounded execution with partial result  
**Actual Behavior**: Stops at MAX_PASSES, returns best-effort PDF  
**User Impact**: Compilation completes, warned about instability

### Scenario: Massive Document (1000+ pages)
**Input**: Very large document exceeding normal limits  
**Expected**: Graceful handling within quota  
**Actual Behavior**: Per-job timeout applied, partial results possible  
**User Impact**: May need to use linear mode or split document

### Scenario: Many Concurrent Users (100+)
**Input**: Burst of compilation requests  
**Expected**: Fair queuing, no starvation  
**Actual Behavior**: Weighted scheduling distributes resources  
**User Impact**: May experience queuing delay, served fairly

### Scenario: Malicious Workload
**Input**: TeX bomb or resource exhaustion attempt  
**Expected**: Quota enforcement, job termination  
**Actual Behavior**: Job killed at quota limit, user warned  
**User Impact**: Compilation fails with clear error message

### Remaining Fundamental Limitations

1. **TeX is single-threaded**: A single large chapter cannot be parallelized
2. **Reference semantics**: Some documents genuinely require many passes
3. **Package compatibility**: Not all packages work in parallel mode
4. **PDF merge quality**: Parallel fragments may have slight rendering differences
5. **Real-time preview**: Compilation latency is unavoidable for complex documents

---

## PHASE 6: Final Consistency Check

### Issue Resolution Matrix

| Issue | Status | Resolution |
|-------|--------|------------|
| Reference convergence | ✓ Bounded | MAX_PASSES with graceful failure |
| Dependency incompleteness | ⚠ Mitigated | Linear mode for correctness |
| Stateful TeX | ✓ Resolved | Compilation mode duality |
| Network paradox | ✓ Resolved | Offline package bundles |
| Resource exhaustion | ✓ Resolved | Quota enforcement |
| Poisoned jobs | ✓ Resolved | Circuit breaker pattern |
| Redis pressure | ✓ Resolved | TTL + eviction policy |
| Worker starvation | ✓ Resolved | Fair scheduling |
| CRDT growth | ✓ Bounded | Compaction triggers |
| Cold start | ⚠ Mitigated | Pre-warming, caching |
| Data loss | ✓ Resolved | Persistence guarantees |
| Observability | ✓ Resolved | Structured logging + tracing |

### Security Verification

- ✓ No new network access introduced
- ✓ Resource limits prevent DoS
- ✓ Failure isolation prevents cascade
- ✓ Quota system prevents abuse
- ✓ No shell escape paths added

### Performance vs Correctness Trade-offs

| Mode | Speed | Correctness | Use Case |
|------|-------|-------------|----------|
| Speculative | Fast | Best-effort | Development, iteration |
| Guaranteed | Slower | Correct | Final submission, publication |

Users explicitly choose their trade-off. System default is speculative for development velocity.

---

## Appendix: System-Level Guarantees Required

The following guarantees MUST be provided by the deployment environment:

1. **Firecracker KVM**: Hardware virtualization for isolation
2. **Redis persistence**: AOF with fsync on write for durability
3. **S3 durability**: 11 9s durability for artifact storage
4. **Network isolation**: VPC with no egress for workers
5. **Monitoring**: Prometheus + Grafana for observability
6. **Alerting**: PagerDuty integration for critical failures

These are infrastructure requirements, not code implementations.
