# Phase 3 MVP — Clipboard DLP (Linux, Luhn, log_only)

**Status:** design 2026-05-21
**Target tag:** v0.3.0
**Scope:** Minimalna pełna pętla DLP od strzała.

## Decyzje

- **Platforma:** Linux only (macOS/Windows iteracyjnie w v0.3.1/2)
- **Classifier:** 1 sztuka — Luhn (numery kart kredytowych)
- **Trigger:** always-on (clipboard monitorowany od startu agenta, polityki filtrują)
- **Transport eventów:** rozszerzenie gRPC Stream (nowy rpc UploadEvents)
- **Privacy:** redacted payload (`****-****-****-9010` + last4 + classifier label)
- **Action:** log_only (enforcement framework = Phase 4)
- **agent_id:** z mTLS Subject CN (jak Phase 1+2) — nie w payload
- **Queue:** sqlite WAL w `/var/lib/scrooge/events.db`, retencja 7 dni
- **Manager events table:** zwykła table + indexy (partycjonowanie YAGNI dla MVP, dorzucamy gdy urośnie)
- **Dashboard:** nowa zakładka Events (tabela + filtry severity/agent/time)

## Pliki / moduły

```
crates/scrooge-agent-core/src/
├── modules/mod.rs              trait Module
├── modules/classifier/{mod,luhn}.rs
├── event.rs                    extend
└── queue.rs                    sqlite WAL

crates/scrooge-agent-linux/src/clipboard.rs

crates/scrooge-agent-bin/src/
├── modules_runtime.rs          orchestracja
└── event_uploader.rs           batch send

crates/scrooge-manager-api/src/handlers/events.rs   REST + gRPC bridge
crates/scrooge-manager-bin/src/grpc.rs              extend UploadEvents

proto/scrooge.proto                                 Event, EventBatch, UploadEvents
migrations/20260522000001_events.sql                events table
crates/scrooge-manager-api/static/index.html        Events tab
```

## Event flow

1. Agent boot → modules_runtime spawn ClipboardModule + ClassifierRegistry (Luhn)
2. Clipboard polling 500ms (arboard) → hash check → text → Classifier.scan → matches
3. Match → Event { ts, source_type=Clipboard, classifier=credit_card, last4, redacted, severity=medium } → sqlite enqueue
4. event_uploader (background, 5s tick) → fetch batch 50 → gRPC UploadEvents → mark_sent
5. Manager rpc handler → PG insert events table → return EventAck { accepted: N }
6. Dashboard Events tab → GET /api/v1/events?... → render tabela

## Schemat proto

```protobuf
message Event {
  google.protobuf.Timestamp ts = 1;
  SourceType source_type = 2;
  string classifier_match = 3;
  string redacted_preview = 4;
  string last4 = 5;
  Severity severity = 6;
  optional string rule_id = 7;
  map<string, string> metadata = 8;
}
message EventBatch { repeated Event events = 1; }
message EventAck { uint32 accepted = 1; }
rpc UploadEvents(EventBatch) returns (EventAck);
```

## Schemat PG

```sql
CREATE TABLE events (
    id BIGSERIAL PRIMARY KEY,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    ts TIMESTAMPTZ NOT NULL,
    source_type TEXT NOT NULL,
    classifier_match TEXT NOT NULL,
    redacted_preview TEXT NOT NULL,
    last4 TEXT NOT NULL,
    severity TEXT NOT NULL,
    rule_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX events_ts_idx ON events (ts DESC);
CREATE INDEX events_agent_ts_idx ON events (agent_id, ts DESC);
CREATE INDEX events_severity_idx ON events (severity);
CREATE INDEX events_classifier_idx ON events (classifier_match);
```

## Plan implementacji (commit-per-step)

1. Migration `events` table
2. proto extend (Event/EventBatch/EventAck/UploadEvents) + regen
3. agent-core: event.rs extend + classifier trait + Luhn impl + tests
4. agent-core: queue.rs sqlite WAL + tests
5. agent-linux: clipboard.rs polling przez arboard
6. agent-bin: modules_runtime + event_uploader
7. manager: gRPC handler UploadEvents
8. manager: REST GET /api/v1/events (filtry: severity, agent_id, time, limit/offset)
9. dashboard: Events tab w index.html
10. E2E manual na żywych VM-kach: kopiuj kartę kredytową na agencie → event w dashboardzie

## Out of scope (post-MVP)

- macOS/Windows clipboard
- PESEL/IBAN/NIP classifiers
- USB / filesystem watchers
- block/quarantine/lock/kill enforce
- Event partycjonowanie + retencja po stronie managera
- WebSocket push do dashboardu (live events)
- Polling/refresh w dashboard events tab (manual refresh button w MVP)
