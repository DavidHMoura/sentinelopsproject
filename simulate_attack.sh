#!/usr/bin/env bash
# simulate_attack.sh — Prova o circuito completo do SentinelOps
#
# FASE 1 (REST): curl → /api/events/ingest → DetectionEngine → PostgreSQL
#                (circuito 100% funcional hoje)
#
# FASE 2 (gRPC): grpcurl → Control Plane (mTLS Zero Trust) → Kafka events.raw
#                (requer certs gerados por este script + Control Plane rodando)
#
# Uso:
#   ./simulate_attack.sh            # gera certs + fase 1 + fase 2
#   ./simulate_attack.sh certs      # só gera certs (copiar para classpath do CP)
#   ./simulate_attack.sh rest       # só fase 1 (REST API)
#   ./simulate_attack.sh grpc       # só fase 2 (gRPC/mTLS)
#   ./simulate_attack.sh monitor    # imprime comandos de monitoramento

set -euo pipefail

# ─── Configuração ─────────────────────────────────────────────────────────────
API_BASE="${SENTINEL_API:-http://localhost:8000}"
CONTROL_PLANE="${CONTROL_PLANE_ADDR:-localhost:9090}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CERTS_DIR="${PROJECT_ROOT}/certs"
PROTO_DIR="${PROJECT_ROOT}/proto"

# Carrega .env se existir (para API_KEYS, DATABASE_URL, etc.)
if [[ -f "${PROJECT_ROOT}/.env" ]]; then
  # shellcheck disable=SC2046
  export $(grep -v '^#' "${PROJECT_ROOT}/.env" | xargs -d '\n')
fi

API_KEY="${API_KEYS%%,*}"   # usa a primeira chave da lista
SIM_AGENT_ID="sim-agent-001"
TLS_SERVER_NAME="sentinelops-control.internal"

# Cores para output legível
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

header()  { echo -e "\n${BOLD}${CYAN}══════════════════════════════════════${RESET}"; echo -e "${BOLD}${CYAN}  $*${RESET}"; echo -e "${BOLD}${CYAN}══════════════════════════════════════${RESET}"; }
info()    { echo -e "${GREEN}[INFO]${RESET} $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
die()     { echo -e "${RED}[ERRO]${RESET} $*" >&2; exit 1; }
ok()      { echo -e "${GREEN}[OK]${RESET} $*"; }

# ─── Pré-requisitos ───────────────────────────────────────────────────────────
check_prereqs() {
  local missing=0
  for cmd in curl openssl jq; do
    if ! command -v "$cmd" &>/dev/null; then
      warn "Comando ausente: $cmd"
      missing=1
    fi
  done
  if [[ "$missing" -eq 1 ]]; then
    die "Instale as dependências ausentes antes de continuar."
  fi

  if [[ -z "${API_KEY:-}" ]]; then
    warn "API_KEY não encontrada no .env — usando 'dev-key-sim' para simulação."
    warn "Verifique se o servidor Rust foi iniciado com API_KEYS=dev-key-sim"
    API_KEY="dev-key-sim"
  fi
}

# ─── Gera certs de desenvolvimento para mTLS ──────────────────────────────────
gen_certs() {
  header "Gerando certificados mTLS de desenvolvimento"
  mkdir -p "${CERTS_DIR}"
  local cp_certs="${PROJECT_ROOT}/sentinelops-control/src/main/resources/certs"
  mkdir -p "${cp_certs}"

  # 1. CA
  if [[ ! -f "${CERTS_DIR}/ca.key" ]]; then
    info "Gerando CA..."
    openssl genrsa -out "${CERTS_DIR}/ca.key" 4096 2>/dev/null
    openssl req -x509 -new -nodes \
      -key "${CERTS_DIR}/ca.key" \
      -sha256 -days 3650 \
      -subj "/CN=SentinelOps-Dev-CA/O=SentinelOps/C=BR" \
      -out "${CERTS_DIR}/ca.pem" 2>/dev/null
    ok "CA gerada em ${CERTS_DIR}/ca.pem"
  else
    info "CA já existe — pulando geração."
  fi

  # 2. Cert do servidor (Control Plane)
  if [[ ! -f "${CERTS_DIR}/server.key" ]]; then
    info "Gerando cert do servidor (SAN: ${TLS_SERVER_NAME}, localhost)..."
    openssl genrsa -out "${CERTS_DIR}/server.key" 2048 2>/dev/null

    cat > "${CERTS_DIR}/server_ext.cnf" <<EOF
[req]
req_extensions     = v3_req
distinguished_name = dn
[dn]
[v3_req]
subjectAltName = @alt_names
[alt_names]
DNS.1 = ${TLS_SERVER_NAME}
DNS.2 = localhost
IP.1  = 127.0.0.1
EOF

    openssl req -new \
      -key "${CERTS_DIR}/server.key" \
      -subj "/CN=${TLS_SERVER_NAME}/O=SentinelOps/C=BR" \
      -out "${CERTS_DIR}/server.csr" 2>/dev/null

    openssl x509 -req \
      -in "${CERTS_DIR}/server.csr" \
      -CA "${CERTS_DIR}/ca.pem" \
      -CAkey "${CERTS_DIR}/ca.key" \
      -CAcreateserial \
      -out "${CERTS_DIR}/server.pem" \
      -days 365 -sha256 \
      -extfile "${CERTS_DIR}/server_ext.cnf" \
      -extensions v3_req 2>/dev/null
    ok "Cert do servidor gerado."
  else
    info "Cert do servidor já existe — pulando."
  fi

  # 3. Cert do agente simulado (CN deve corresponder ao agent_id)
  if [[ ! -f "${CERTS_DIR}/${SIM_AGENT_ID}.key" ]]; then
    info "Gerando cert do agente simulado (CN=${SIM_AGENT_ID})..."
    openssl genrsa -out "${CERTS_DIR}/${SIM_AGENT_ID}.key" 2048 2>/dev/null
    openssl req -new \
      -key "${CERTS_DIR}/${SIM_AGENT_ID}.key" \
      -subj "/CN=${SIM_AGENT_ID}/O=SentinelOps-Agent/C=BR" \
      -out "${CERTS_DIR}/${SIM_AGENT_ID}.csr" 2>/dev/null
    openssl x509 -req \
      -in "${CERTS_DIR}/${SIM_AGENT_ID}.csr" \
      -CA "${CERTS_DIR}/ca.pem" \
      -CAkey "${CERTS_DIR}/ca.key" \
      -CAcreateserial \
      -out "${CERTS_DIR}/${SIM_AGENT_ID}.pem" \
      -days 365 -sha256 2>/dev/null
    ok "Cert do agente gerado: CN=${SIM_AGENT_ID}"
  else
    info "Cert do agente já existe — pulando."
  fi

  # Copia certs para o classpath do Control Plane
  info "Copiando certs para ${cp_certs}/ ..."
  cp "${CERTS_DIR}/ca.pem"     "${cp_certs}/ca.pem"
  cp "${CERTS_DIR}/server.pem" "${cp_certs}/server.pem"
  cp "${CERTS_DIR}/server.key" "${cp_certs}/server.key"
  ok "Certs copiados para o classpath do Control Plane."
  warn "IMPORTANTE: reinicie o Control Plane para que ele carregue os novos certs."
  warn "  cd sentinelops-control && mvn spring-boot:run -Dspring-boot.run.profiles=dev"
}

# ─── FASE 1: REST API → Detection Engine → PostgreSQL ────────────────────────
phase1_rest() {
  header "FASE 1 — REST API: Simulando ataques (circuito completo)"

  # Verifica se o servidor Rust está respondendo
  if ! curl -sf "${API_BASE}/api/events" -H "X-API-Key: ${API_KEY}" -o /dev/null; then
    die "Servidor Rust não está respondendo em ${API_BASE}. Inicie com: cargo run"
  fi
  ok "Servidor Rust acessível em ${API_BASE}"

  # ── 1.1 Brute-Force (auth.login.failed) ────────────────────────────────────
  echo ""
  info "Injetando 12 tentativas de login falhadas (threshold: 10)..."
  info "IP atacante: 10.99.0.1  |  Actor: attacker@evil.com"
  local ts
  for i in $(seq 1 12); do
    ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    local status
    status=$(curl -sf -o /dev/null -w "%{http_code}" \
      -X POST "${API_BASE}/api/events/ingest" \
      -H "Content-Type: application/json" \
      -H "X-API-Key: ${API_KEY}" \
      -d "{
        \"ts\": \"${ts}\",
        \"event_type\": \"auth.login.failed\",
        \"source_ip\": \"10.99.0.1\",
        \"actor\": \"attacker@evil.com\",
        \"meta\": {\"attempt\": ${i}, \"service\": \"ssh\"}
      }")
    if [[ "$status" == "202" ]]; then
      printf "  tentativa %2d/12 → HTTP %s\n" "$i" "$status"
    else
      warn "tentativa $i devolveu HTTP $status"
    fi
    sleep 0.1
  done

  # ── 1.2 Port Scan (network.scan) ───────────────────────────────────────────
  echo ""
  info "Injetando 25 port-scan events (threshold: 20, janela: 10s)..."
  info "IP atacante: 10.99.0.2  |  Portas distintas: 1-25"
  info "Todos os eventos enviados em rajada para caber na janela de 10s..."
  for port in $(seq 1 25); do
    ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    local status
    status=$(curl -sf -o /dev/null -w "%{http_code}" \
      -X POST "${API_BASE}/api/events/ingest" \
      -H "Content-Type: application/json" \
      -H "X-API-Key: ${API_KEY}" \
      -d "{
        \"ts\": \"${ts}\",
        \"event_type\": \"network.scan\",
        \"source_ip\": \"10.99.0.2\",
        \"actor\": null,
        \"meta\": {\"target_port\": ${port}, \"protocol\": \"tcp\"}
      }")
    if [[ "$status" == "202" ]]; then
      printf "  porta %3d/25 → HTTP %s\n" "$port" "$status"
    else
      warn "porta $port devolveu HTTP $status"
    fi
  done

  # ── 1.3 Verifica alertas no PostgreSQL ────────────────────────────────────
  echo ""
  info "Aguardando flush do ingestor (3s)..."
  sleep 3

  header "Alertas gerados no PostgreSQL"
  local alerts
  alerts=$(curl -sf "${API_BASE}/api/alerts" -H "X-API-Key: ${API_KEY}")
  echo "${alerts}" | jq -r '
    .[] |
    "\(.severity | ascii_upcase) | \(.title)\n  → \(.description)\n  → fingerprint: \(.fingerprint)\n"
  ' 2>/dev/null || echo "${alerts}" | jq '.'

  local count
  count=$(echo "${alerts}" | jq 'length')
  echo ""
  if [[ "$count" -gt 0 ]]; then
    ok "Total de ${count} alerta(s) detectado(s) e persistido(s) no PostgreSQL."
  else
    warn "Nenhum alerta ainda — verifique se o servidor está com o mesmo estado de memória (sem reinicialização entre os eventos)."
  fi
}

# ─── FASE 2: gRPC / mTLS → Control Plane → Kafka ─────────────────────────────
phase2_grpc() {
  header "FASE 2 — gRPC/mTLS: Zero Trust → Control Plane → Kafka"

  # Verifica certs
  if [[ ! -f "${CERTS_DIR}/ca.pem" ]] || [[ ! -f "${CERTS_DIR}/${SIM_AGENT_ID}.pem" ]]; then
    warn "Certs não encontrados. Gerando agora..."
    gen_certs
  fi

  # Detecta grpcurl (instalado ou via Docker)
  local grpcurl_bin=""
  if command -v grpcurl &>/dev/null; then
    grpcurl_bin="grpcurl"
    ok "grpcurl encontrado: $(command -v grpcurl)"
  elif docker image inspect fullstorydev/grpcurl &>/dev/null 2>&1 || \
       docker pull fullstorydev/grpcurl:latest &>/dev/null 2>&1; then
    grpcurl_bin="docker"
    ok "Usando grpcurl via Docker (fullstorydev/grpcurl)."
  else
    warn "grpcurl não encontrado (nem local nem via Docker)."
    echo ""
    echo "  Opções de instalação:"
    echo "    # Fedora/RHEL:"
    echo "    sudo dnf install grpcurl"
    echo ""
    echo "    # Go:"
    echo "    go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest"
    echo ""
    echo "    # Docker (sem instalação):"
    echo "    docker pull fullstorydev/grpcurl"
    echo ""
    echo "  Após instalar, reexecute:"
    echo "    ./simulate_attack.sh grpc"
    return 0
  fi

  # Função wrapper: chama grpcurl direto ou via Docker
  run_grpcurl() {
    if [[ "$grpcurl_bin" == "grpcurl" ]]; then
      grpcurl "$@"
    else
      docker run --rm --network host \
        -v "${CERTS_DIR}:/certs:ro" \
        -v "${PROTO_DIR}:/proto:ro" \
        fullstorydev/grpcurl "$@"
    fi
  }

  # Ajusta paths de certs se usando Docker (paths internos ao container)
  local ca_path cert_path key_path proto_path
  if [[ "$grpcurl_bin" == "docker" ]]; then
    ca_path="/certs/ca.pem"
    cert_path="/certs/${SIM_AGENT_ID}.pem"
    key_path="/certs/${SIM_AGENT_ID}.key"
    proto_path="/proto/sentinel.proto"
  else
    ca_path="${CERTS_DIR}/ca.pem"
    cert_path="${CERTS_DIR}/${SIM_AGENT_ID}.pem"
    key_path="${CERTS_DIR}/${SIM_AGENT_ID}.key"
    proto_path="${PROTO_DIR}/sentinel.proto"
  fi

  # Verifica se o Control Plane está no ar
  info "Verificando Control Plane em ${CONTROL_PLANE}..."
  if ! run_grpcurl \
    -cacert "${ca_path}" \
    -cert "${cert_path}" \
    -key "${key_path}" \
    -servername "${TLS_SERVER_NAME}" \
    -proto "${proto_path}" \
    "${CONTROL_PLANE}" list 2>/dev/null | grep -q "sentinel.IngestionService"; then
    warn "Control Plane não acessível ou serviço não listado."
    warn "Inicie o Control Plane:"
    warn "  cd sentinelops-control && mvn spring-boot:run"
    warn "(Certifique-se de ter rodado './simulate_attack.sh certs' primeiro)"
    return 0
  fi
  ok "Control Plane acessível. Serviço sentinel.IngestionService encontrado."

  # ── 2.1 SendEvent: brute-force event via gRPC ──────────────────────────────
  echo ""
  info "Enviando evento auth.login.failed via gRPC/mTLS (SendEvent)..."
  info "agent_id: ${SIM_AGENT_ID}  |  cert CN: ${SIM_AGENT_ID}  |  Zero Trust: PASS esperado"

  local evt_id="grpc-sim-$(date +%s)"
  local ts
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  local response
  response=$(run_grpcurl \
    -cacert "${ca_path}" \
    -cert "${cert_path}" \
    -key "${key_path}" \
    -servername "${TLS_SERVER_NAME}" \
    -proto "${proto_path}" \
    -d "{
      \"event_id\":   \"${evt_id}\",
      \"timestamp\":  \"${ts}\",
      \"event_type\": \"auth.login.failed\",
      \"source_ip\":  \"10.99.0.3\",
      \"agent_id\":   \"${SIM_AGENT_ID}\",
      \"source_host\": \"sim-host-001\"
    }" \
    "${CONTROL_PLANE}" sentinel.IngestionService/SendEvent)

  echo "  Resposta gRPC: ${response}"

  if echo "${response}" | grep -q '"accepted": true'; then
    ok "Evento aceito pelo Control Plane — publicado em Kafka events.raw!"
    echo ""
    info "Para verificar no Kafka (Redpanda):"
    echo "  docker compose exec redpanda rpk topic consume events.raw --offset start -n 5"
  else
    warn "Evento não aceito. Verifique os logs do Control Plane."
  fi

  # ── 2.2 Teste de rejeição Zero Trust ─────────────────────────────────────
  echo ""
  info "Testando rejeição Zero Trust: agent_id != cert CN..."
  info "Enviando agent_id='impostor-agent' com cert CN='${SIM_AGENT_ID}' — deve ser rejeitado."

  local reject_response
  reject_response=$(run_grpcurl \
    -cacert "${ca_path}" \
    -cert "${cert_path}" \
    -key "${key_path}" \
    -servername "${TLS_SERVER_NAME}" \
    -proto "${proto_path}" \
    -d "{
      \"event_id\":   \"grpc-reject-$(date +%s)\",
      \"timestamp\":  \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
      \"event_type\": \"network.scan\",
      \"source_ip\":  \"10.99.0.4\",
      \"agent_id\":   \"impostor-agent\",
      \"source_host\": \"evil-host\"
    }" \
    "${CONTROL_PLANE}" sentinel.IngestionService/SendEvent 2>&1 || true)

  if echo "${reject_response}" | grep -qiE "PERMISSION_DENIED|rejected|impostor"; then
    ok "Zero Trust funcionando: impostor rejeitado pelo Control Plane."
  else
    echo "  Resposta: ${reject_response}"
    warn "Resposta inesperada — verifique os logs do Control Plane."
  fi
}

# ─── Monitoramento ────────────────────────────────────────────────────────────
show_monitor() {
  header "Comandos de monitoramento (abrir em terminais separados)"

  echo -e "\n${BOLD}Terminal 1 — Rust Detection Engine (logs em tempo real):${RESET}"
  echo "  cargo run"
  echo "  # ou se já rodando, tail do processo"

  echo -e "\n${BOLD}Terminal 2 — Kafka: consumir events.raw (circuito gRPC):${RESET}"
  echo "  docker compose exec redpanda rpk topic consume events.raw --offset start"

  echo -e "\n${BOLD}Terminal 3 — Control Plane (logs gRPC):${RESET}"
  echo "  cd sentinelops-control && mvn spring-boot:run"

  echo -e "\n${BOLD}Terminal 4 — PostgreSQL: alertas em tempo real:${RESET}"
  echo "  docker compose exec postgres psql -U sentinelops -d sentinelops \\"
  echo "    -c 'SELECT severity, title, description, created_at FROM alerts ORDER BY created_at DESC LIMIT 10;'"

  echo -e "\n${BOLD}Verificação rápida via API REST:${RESET}"
  echo "  # Alertas:"
  echo "  curl -s http://localhost:8000/api/alerts -H 'X-API-Key: \${API_KEYS}' | jq '.[].title'"
  echo "  # Eventos:"
  echo "  curl -s http://localhost:8000/api/events -H 'X-API-Key: \${API_KEYS}' | jq 'length'"

  echo -e "\n${BOLD}Kafka — listar tópicos e checar lag:${RESET}"
  echo "  docker compose exec redpanda rpk topic list"
  echo "  docker compose exec redpanda rpk topic describe events.raw"
}

# ─── Main ─────────────────────────────────────────────────────────────────────
main() {
  local cmd="${1:-all}"

  echo -e "${BOLD}SentinelOps Attack Simulator${RESET}"
  echo -e "Projeto: ${PROJECT_ROOT}"
  echo -e "Alvo REST: ${API_BASE} | Alvo gRPC: ${CONTROL_PLANE}"
  echo ""

  case "$cmd" in
    certs)
      check_prereqs
      gen_certs
      ;;
    rest)
      check_prereqs
      phase1_rest
      ;;
    grpc)
      check_prereqs
      phase2_grpc
      ;;
    monitor)
      show_monitor
      ;;
    all)
      check_prereqs
      gen_certs
      echo ""
      phase1_rest
      echo ""
      phase2_grpc
      echo ""
      show_monitor
      ;;
    *)
      echo "Uso: $0 [certs|rest|grpc|monitor|all]"
      exit 1
      ;;
  esac

  echo ""
  ok "Simulação concluída."
}

main "$@"
