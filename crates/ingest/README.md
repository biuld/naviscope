# naviscope-ingest

## 1. Definition

`naviscope-ingest` is a DAG execution runtime built on a message queue.

It drives task transitions between `runnable` and `deferred` via dependencies (`depends_on`), and uses `epoch` for batch completion semantics.

- Component type: runtime middleware
- Input: `Message<P>`
- Output: committed operation results through `CommitSink<Op>`
- Goal: stable dependency-aware execution and batch completion under high concurrency

## 2. Responsibility Boundaries

### 2.1 In Scope

- Message intake and backpressure control
- Deferred persistence and replay for unresolved dependencies
- Dependency-ready event dispatch (`message` / `resource`)
- Batch (`epoch`) completion lifecycle
- Unified runtime observability hooks

### 2.2 Out of Scope

- Business semantics
- Source scanning and incremental discovery
- Concrete storage engine strategy

## 3. Architecture Model

```mermaid
flowchart LR
    P[Producer] --> IH[IntakeHandle]
    IH --> IN[Intake Stream]

    IN --> G{depends_on empty?}
    G -->|yes| EX[Executor]
    G -->|no| DQ[Deferred Queue]

    EX -->|Executed| CM[CommitSink]
    EX -->|Deferred| DQ
    EX -->|Fatal| ER[Runtime Error]

    CM --> DR[Dependency Ready]
    DR --> DS[DeferredStore]
    DQ --> DS
    DS --> RP[Replay pop_ready]
    RP --> IN

    IH --> EP[EpochTracker]
    EX --> EP
    CM --> EP
```

## 4. Runtime Layers

```mermaid
flowchart TB
    subgraph API["API Layer"]
      A1[IntakeHandle]
      A2[notify_dependency_ready]
      A3[new/seal/wait epoch]
    end

    subgraph CORE["Runtime Core"]
      C1[Kernel Event Loop]
      C2[Flow Control]
      C3[Worker Pool]
    end

    subgraph SPI["Extension SPI"]
      S1[Executor]
      S2[DeferredStore]
      S3[CommitSink]
      S4[RuntimeMetrics]
      S5[PipelineBus]
    end

    API --> CORE
    CORE --> SPI
```

## 5. Lifecycle Model

### 5.1 Message Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Ingested
    Ingested --> Runnable: no dependency
    Ingested --> Deferred: dependency unresolved
    Deferred --> Runnable: dependency ready
    Runnable --> Executed
    Runnable --> Deferred: executor defers
    Runnable --> Fatal
    Executed --> Committed
    Committed --> [*]
    Fatal --> [*]
```

### 5.2 Epoch Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Open
    Open --> Open: submit / internal derived
    Open --> Sealed: seal_epoch
    Sealed --> Completed: committed >= submitted
    Completed --> [*]
```

## 6. Processing Sequence

```mermaid
sequenceDiagram
    participant U as Upstream
    participant H as IntakeHandle
    participant K as Kernel
    participant X as Executor
    participant C as CommitSink
    participant S as DeferredStore
    participant E as EpochTracker

    U->>H: new_epoch
    U->>H: submit(messages)
    H->>E: submitted += n
    H->>K: enqueue
    U->>H: seal_epoch

    K->>X: execute runnable
    X-->>K: Executed / Deferred / Fatal
    K->>S: persist deferred
    K->>C: commit executed
    C-->>K: commit ok
    K->>S: notify_ready(message/resource)
    K->>E: committed += m
    S-->>K: replay ready messages

    U->>H: wait_epoch
    H-->>U: return when completed
```

## 7. Extension Contracts (SPI)

- `Executor<P, Op>`: business execution and event emission
- `DeferredStore<P>`: deferred storage, readiness evaluation, and replay source
- `CommitSink<Op>`: commit boundary and visibility control
- `RuntimeMetrics`: metrics collection
- `PipelineBus<P, Op>`: channel model abstraction

## 8. Semantic Guarantees

- Processing semantics: at-least-once
- Idempotency: `Executor` and `CommitSink` should provide idempotency as needed
- Epoch completion: after `seal_epoch`, completion is `committed >= submitted`
- Internally derived deferred messages are counted into the same epoch's `submitted`

## 9. Integration Checklist

1. Assemble `RuntimeComponents` (`Executor` / `DeferredStore` / `CommitSink` / `RuntimeMetrics` / `PipelineBus`).
2. Start `IngestRuntime::run_forever()`.
3. Submit batches through `IntakeHandle` and wait with epoch APIs.
