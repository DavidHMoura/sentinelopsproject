# SentinelOps

Sistema de detecção de intrusão (IDS) com arquitetura event-driven, construído com Rust e Java.

## Arquitetura

```
┌─────────────────────────────────────────────────────────────────┐
│                         Agentes (Rust)                          │
│            Coletam eventos do OS e enviam via gRPC/mTLS         │
└──────────────────────────┬──────────────────────────────────────┘
                           │ gRPC + mTLS (Zero Trust)
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│               Control Plane (Java · Spring Boot 21)             │
│   Valida identidade do agente (cert CN) → publica no Kafka      │
└──────────────────────────┬──────────────────────────────────────┘
                           │ Kafka (Redpanda) · tópico events.raw
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│               Detection Engine (Rust · Actix-Web)               │
│   Consome events.raw → detecta ameaças → persiste no PostgreSQL │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
                      PostgreSQL
                  (eventos · alertas · ML features)
```

### Componentes

| Componente | Stack | Responsabilidade |
|---|---|---|
| `sentinelops-agent` | Rust + tonic | Coleta eventos do OS, envia via gRPC/mTLS |
| `sentinelops-control` | Java 21 + Spring Boot + gRPC | Valida agentes (Zero Trust), publica no Kafka |
| `sentinelops-rust` | Rust + Actix-Web | REST API, Detection Engine, persistência PostgreSQL |
| `proto/` | Protobuf | Contrato gRPC compartilhado |
| `migrations/` | SQLx | Schema PostgreSQL versionado |

---

## Início Rápido

### Pré-requisitos

- Docker + Docker Compose v2
- Rust 1.75+
- Java 21+, Maven 3.9+

### 1. Subir a infraestrutura

```bash
cp .env.example .env          # credenciais dev (não commitar o .env)
docker compose up -d
docker compose ps             # aguardar postgres e redpanda ficarem healthy
```

Portas expostas:

| Porta | Serviço |
|---|---|
| `5432` | PostgreSQL |
| `19092` | Kafka API (host → Redpanda) |
| `9644` | Redpanda Admin API |
| `8082` | Pandaproxy REST |

### 2. Executar os testes

```bash
# Rust (Detection Engine + Agent)
cargo test

# Java (Control Plane)
cd sentinelops-control && mvn test
```

### 3. Rodar o servidor HTTP

```bash
# Requer DATABASE_URL e API_KEYS no .env
cargo run
```

---

## Variáveis de Ambiente

### Rust HTTP Server (`sentinelops-rust`)

| Variável | Descrição | Padrão |
|---|---|---|
| `DATABASE_URL` | PostgreSQL connection string | **Obrigatório** |
| `API_KEYS` | Chaves de API separadas por vírgula | **Obrigatório** |
| `SERVER_HOST` | Endereço de bind | `127.0.0.1` |
| `SERVER_PORT` | Porta | `8000` |
| `AUTH_MAX_ATTEMPTS` | Threshold brute-force | `10` |
| `AUTH_WINDOW_SECONDS` | Janela de detecção (s) | `300` |
| `PORT_SCAN_MAX_PORTS` | Threshold port-scan | `20` |
| `PORT_SCAN_WINDOW_SECONDS` | Janela de detecção port-scan (s) | `10` |
| `INGESTOR_BATCH_SIZE` | Eventos por batch no DB | `100` |
| `INGESTOR_FLUSH_MS` | Intervalo de flush (ms) | `3000` |

### Control Plane (`sentinelops-control`)

| Variável | Descrição | Padrão |
|---|---|---|
| `KAFKA_BOOTSTRAP_SERVERS` | Endereço do broker Kafka | `localhost:19092` |

---

## API REST

Todas as requisições requerem o header `X-API-Key`.

### POST /api/events/ingest

```bash
curl -X POST http://localhost:8000/api/events/ingest \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-key" \
  -d '{
    "ts": "2026-03-21T17:00:00Z",
    "event_type": "auth.login.failed",
    "source_ip": "192.168.1.100",
    "actor": "user@example.com",
    "meta": {}
  }'
```

### GET /api/events

```bash
curl -H "X-API-Key: your-key" http://localhost:8000/api/events
```

### GET /api/alerts

```bash
curl -H "X-API-Key: your-key" http://localhost:8000/api/alerts
```

---

## Regras de Detecção

| Regra | Evento | Condição | Severidade |
|---|---|---|---|
| Brute-force | `auth.login.failed` | ≥ `AUTH_MAX_ATTEMPTS` tentativas na janela | `high` |
| Port scan | `network.scan` | ≥ `PORT_SCAN_MAX_PORTS` portas distintas na janela | `critical` |

---

## Zero Trust (mTLS)

O Control Plane rejeita qualquer chamada gRPC sem certificado de cliente válido. O `AgentIdentityInterceptor` extrai o CN do certificado e o `IngestionServiceImpl` valida que o campo `agent_id` de cada evento corresponde ao CN — impedindo spoofing mesmo com conexão estabelecida.

---

## Estrutura do Repositório

```
sentinelops/
├── src/                          # Detection Engine + REST API (Rust)
│   ├── api.rs
│   ├── detection.rs
│   ├── ingestor.rs
│   ├── ml_features.rs
│   └── middleware/auth.rs
├── sentinelops-agent/            # Agente gRPC (Rust)
│   └── src/
│       ├── client.rs
│       └── collector.rs
├── sentinelops-control/          # Control Plane (Java)
│   └── src/main/java/com/sentinelops/
│       ├── application/port/     # Ports (interfaces)
│       ├── grpc/                 # Handlers gRPC
│       └── infrastructure/kafka/ # Adapters Kafka
├── proto/
│   └── sentinel.proto
├── migrations/
├── docker-compose.yml
└── .env.example
```

---

## Testes

```
Java  (sentinelops-control):  10 testes — AgentIdentityInterceptor, IngestionServiceImpl, KafkaEventPublisher
Rust  (sentinelops-rust):     10 testes — config, models, middleware, detection
Rust  (sentinelops-agent):     9 testes — config, collector
Integration:                   2 testes — brute-force threshold, non-auth events
```

---

## Roadmap

- [ ] Rust Kafka Consumer — fechar o circuito EDA end-to-end
- [ ] Rate limiting ativo nas rotas REST
- [ ] Alert routing (webhook / Slack)
- [ ] Collector real no agente (eBPF / auditd)
- [ ] Métricas Prometheus

---

## Licença

MIT

## Contato

David Moura — [github.com/DavidHMoura/sentinelopsproject](https://github.com/DavidHMoura/sentinelopsproject)
