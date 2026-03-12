# SentinelOps Rust - Evolução v0.1.0 → v0.2.0

## Objetivo desta Evolução

Esta versão implementa as **4 melhorias críticas da Fase 1** que são essenciais para demonstrar maturidade em desenvolvimento Rust e consciência de segurança em entrevistas técnicas:

1. **Autenticação via API Key**
2. **Rate Limiting**
3. **Tratamento de Erros Robusto**
4. **Testes Unitários e de Integração**

---

## Detalhamento das Mudanças

### 1. Sistema de Erros Customizado (`src/errors.rs`)

**Por que é importante:**
- Em Rust, tratamento de erros é um diferencial técnico crucial
- Demonstra compreensão de tipos, enums e traits
- Elimina `.unwrap()` que pode causar panics em produção

**O que foi feito:**

```rust
#[derive(Error, Debug)]
pub enum SentinelError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Authentication failed: {0}")]
    AuthError(String),
    
    #[error("Invalid input: {0}")]
    ValidationError(String),
    
    // ... outros erros
}
```

**Benefícios:**
- Erros tipados e auto-documentados
- Conversão automática de erros (operator `?`)
- Respostas HTTP padronizadas (implementação de `ResponseError`)
- Type alias `SentinelResult<T>` para reduzir verbosidade

**Exemplos de uso:**
```rust
// ANTES (v0.1.0):
let pool = create_pool(&url).await.unwrap(); 

// DEPOIS (v0.2.0):
let pool = create_pool(&url).await?;
```

---

### 2. Middleware de Autenticação (`src/middleware/auth.rs`)

**Como funciona:**

1. **Configuração** (`.env`):
```bash
API_KEYS=key-dev-001,key-prod-abc123
```

2. **Verificação** (middleware):
```rust
// Extrai header X-API-Key
let api_key = req.headers().get("X-API-Key");

// Valida contra lista de chaves autorizadas
if !valid_keys.contains(api_key) {
    return Unauthorized(401);
}
```

3. **Uso** (requisição):
```bash
curl -H "X-API-Key: key-dev-001" http://localhost:8000/api/events
```

**Features:**
- Suporte a múltiplas API keys
- Logging de tentativas de autenticação falhas
- Testes unitários incluídos
- Respostas HTTP padronizadas

**Implementação do Trait `Transform`:**
```rust
impl<S, B> Transform<S, ServiceRequest> for ApiKeyAuth {
    // Implementação do middleware Actix
}
```
Isso demonstra conhecimento avançado de traits e generics em Rust.

---

### 3. Rate Limiting (`actix-governor`) 🛡️

**Por que é importante:**
- Previne ataques DoS
- Mostra consciência de resiliência de sistemas
- Padrão de mercado em APIs públicas

**Configuração:**
```rust
let governor_conf = GovernorConfigBuilder::default()
    .per_second(2)  // 2 requisições/segundo
    .burst_size(10) // até 10 em burst
    .finish()?;
```

**Comportamento:**
- Limite por IP do cliente
- Retorna HTTP 429 (Too Many Requests) quando excedido
- Não afeta requisições autenticadas normais

**Por que esses valores:**
- 2 req/s = 120 req/min (suficiente para maioria dos casos)
- Burst de 10 permite picos ocasionais sem bloquear usuários legítimos

---

### 4. Validação de Entrada (`validator`) ✔️

**Por que é importante:**
- Previne injeção de dados malformados
- Demonstra princípios de defensive programming
- Valida dados ANTES de processar

**Implementação:**
```rust
#[derive(Deserialize, Validate)]
pub struct EventIn {
    #[validate(length(min = 1, max = 100))]
    pub event_type: String,
    
    #[validate(length(min = 7, max = 45))]
    pub source_ip: String,
    
    // ... outros campos
}
```

**Validação no handler:**
```rust
payload.validate().map_err(|e| {
    SentinelError::ValidationError(format!("Invalid event data: {}", e))
})?;
```

**Mensagens de erro claras:**
```json
{
  "error": "Invalid input: event_type must be between 1 and 100 characters",
  "status": 400
}
```

---

### 5. Logging Estruturado (`tracing`) 📊

**Por que é importante:**
- `log` crate é básico, `tracing` é padrão moderno
- Suporte a spans e contextos (crucial para debugging distribuído)
- Melhor performance e mais features

**ANTES (v0.1.0):**
```rust
log::info!("Starting server on 127.0.0.1:8000");
```

**DEPOIS (v0.2.0):**
```rust
tracing::info!(
    event_id = %event.id,
    event_type = %event.event_type,
    source_ip = %event.source_ip,
    "Ingesting new event"
);
```

**Benefícios:**
- Logs estruturados (fácil de parsear)
- Contexto rico para debugging
- Integração com ferramentas de observabilidade (Jaeger, Datadog)

**Output:**
```
2026-03-08T22:00:00Z INFO Ingesting new event event_id=123e4567-e89b-12d3-a456-426614174000 event_type=auth.login.failed source_ip=192.168.1.1
```

---

### 6. Configuração Robusta (`src/config.rs`) ⚙️

**ANTES (v0.1.0):**
```rust
auth_max_attempts: env::var("AUTH_MAX_ATTEMPTS")
    .unwrap_or_else(|_| "10".to_string())
    .parse()
    .unwrap() // ❌ Panic se não for número!
```

**DEPOIS (v0.2.0):**
```rust
let auth_max_attempts = env::var("AUTH_MAX_ATTEMPTS")
    .unwrap_or_else(|_| "10".to_string())
    .parse()
    .map_err(|e| {
        SentinelError::ConfigError(format!("Invalid AUTH_MAX_ATTEMPTS: {}", e))
    })?; // ✅ Erro explícito com contexto
```

**Features adicionais:**
- Validação de API_KEYS obrigatória
- Parsing robusto de porta e host
- Mensagens de erro descritivas
- Logging de configuração carregada

---

### 7. Testes (`tests/detection_integration.rs`) 🧪

**Por que é importante:**
- Confiança em mudanças futuras
- Demonstra disciplina de engenharia
- Facilita onboarding de novos devs

**Tipos de testes implementados:**

#### a) Testes Unitários (módulo de middleware)
```rust
#[actix_rt::test]
async fn test_valid_api_key() {
    // Testa middleware com chave válida
}

#[actix_rt::test]
async fn test_invalid_api_key() {
    // Testa middleware com chave inválida
}
```

#### b) Testes de Integração (detecção)
```rust
#[actix_rt::test]
#[ignore] // Requer banco de dados
async fn test_bruteforce_detection_threshold() {
    // Testa lógica de detecção com banco real
}
```

**Como rodar:**
```bash
# Testes unitários
cargo test

# Testes de integração (requer PostgreSQL)
cargo test --test detection_integration -- --ignored
```

---

## 🔧 Mudanças na Estrutura do Projeto

### Novos arquivos:
```
src/
├── errors.rs           ✨ NOVO - Sistema de erros
├── middleware/         ✨ NOVO - Middlewares
│   ├── mod.rs
│   └── auth.rs        ✨ NOVO - Autenticação
tests/
└── detection_integration.rs ✨ NOVO - Testes
.env.example            ✨ NOVO - Template de config
```

### Arquivos modificados:
```
Cargo.toml              ⚡ Novas dependências
src/main.rs            ⚡ Integração de middlewares
src/config.rs          ⚡ Tratamento de erros robusto
src/db.rs              ⚡ Usa SentinelResult
src/api.rs             ⚡ Validação e erros
src/models.rs          ⚡ Validação com validator
src/detection.rs       ⚡ Logging estruturado
```

---

## 📦 Novas Dependências

```toml
# Logging estruturado (substitui log + env_logger)
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Erros customizados
thiserror = "1.0"

# Rate limiting
actix-governor = "0.5"

# Validação de dados
validator = { version = "0.18", features = ["derive"] }

# Testes
[dev-dependencies]
actix-rt = "2.9"
```

---

## 🚀 Como Usar a Nova Versão

### 1. Configuração Inicial

```bash
# Copie o template de configuração
cp .env.example .env

# IMPORTANTE: Edite .env e configure suas API keys
# Em produção, use valores seguros e únicos!
nano .env
```

### 2. Rode o Servidor

```bash
cargo run
```

**Output esperado:**
```
🚀 Starting SentinelOps Rust v0.2.0
Configuration loaded: 2 API keys configured
Creating database connection pool...
Database connection pool created successfully
Running database migrations...
Database migrations completed successfully
🔒 Security features enabled:
  ✓ API Key authentication
  ✓ Rate limiting (2 req/s, burst 10)
  ✓ Input validation
  ✓ Structured logging
🌐 Starting server on 127.0.0.1:8000
```

### 3. Teste com curl

```bash
# ❌ Sem autenticação (401 Unauthorized)
curl http://localhost:8000/api/events

# ✅ Com autenticação válida
curl -H "X-API-Key: sentinel-dev-key-001" \
     http://localhost:8000/api/events

# ✅ Ingestão de evento
curl -X POST http://localhost:8000/api/events/ingest \
     -H "Content-Type: application/json" \
     -H "X-API-Key: sentinel-dev-key-001" \
     -d '{
       "ts": "2026-03-08T22:00:00Z",
       "event_type": "auth.login.failed",
       "source_ip": "192.168.1.100",
       "actor": "user@example.com",
       "meta": {}
     }'
```

### 4. Rodando Testes

```bash
# Testes unitários (não requerem banco)
cargo test

# Testes de integração (requerem PostgreSQL rodando)
cargo test --test detection_integration -- --ignored --test-threads=1
```

---

## 💡 Respondendo Perguntas em Entrevistas

### "Como você protege sua API?"

> "Implementei autenticação via API Key usando um middleware customizado em Actix-web. O middleware valida o header `X-API-Key` contra uma lista configurável de chaves autorizadas. Além disso, adicionei rate limiting com `actix-governor` para prevenir abuso, configurado para 2 req/s com burst de 10. Todos os endpoints são protegidos por default, e tentativas de autenticação falhas são logadas para auditoria."

### "Como você lida com erros em Rust?"

> "Criei um enum customizado `SentinelError` usando `thiserror` que mapeia todos os possíveis erros da aplicação. Implementei o trait `ResponseError` do Actix para converter automaticamente erros em respostas HTTP apropriadas. Eliminei todos os `.unwrap()` do código, substituindo por propagação adequada de erros com o operator `?`. Isso garante que erros sejam tratados de forma previsível e que nunca tenhamos panics em produção."

### "Seu código tem testes?"

> "Sim, implementei duas camadas de testes. Testes unitários para componentes isolados como o middleware de autenticação, que validam comportamento sem dependências externas. E testes de integração para a lógica de detecção, que usam um banco de dados real para validar o fluxo completo. Os testes cobrem casos de sucesso, falha e edge cases."

### "Como você loga informações do sistema?"

> "Uso `tracing` ao invés de `log`, que é o padrão moderno em Rust. Tracing permite logging estruturado com contexto rico, facilitando debugging e integração com ferramentas de observabilidade. Por exemplo, ao ingerir um evento, logo o `event_id`, `event_type` e `source_ip` como campos estruturados, não apenas como texto."

### "Como você valida dados de entrada?"

> "Uso a crate `validator` com derive macros para validar dados de entrada declarativamente. Por exemplo, `EventIn` tem validações de comprimento para `event_type` e `source_ip`. A validação acontece antes de qualquer processamento, retornando HTTP 400 com mensagem descritiva se os dados forem inválidos. Isso previne que dados malformados cheguem à lógica de negócio ou ao banco de dados."

---

## 🎓 Conceitos Avançados de Rust Demonstrados

### 1. **Traits e Generics**
- Implementação de `Transform` para middleware
- Implementação de `ResponseError` para erros
- Uso de `#[from]` em `thiserror` para conversão automática

### 2. **Ownership e Borrowing**
- Pool de conexões com `Arc` implícito (via `web::Data`)
- Clonagem estratégica de `Rc<Vec<String>>` para API keys

### 3. **Async/Await**
- Futures e `LocalBoxFuture` no middleware
- Uso correto de `async move` e `Box::pin`

### 4. **Error Handling**
- Operator `?` para propagação
- `Result` com tipos customizados
- `map_err` para conversão de erros

### 5. **Derive Macros**
- `#[derive(Error)]` do thiserror
- `#[derive(Validate)]` do validator
- Combinação de derives do serde + sqlx

---

## 🔥 Próximos Passos (Fase 2 - Opcional)

Quando você estiver confortável com essas melhorias, considere:

1. **Motor de regras configurável** (JSON/YAML)
2. **Múltiplos tipos de detecção** (port scanning, anomalias)
3. **Enriquecimento de eventos** (GeoIP, threat intel)
4. **Notificações** (webhook, email)
5. **Dashboard web** (opcional, mas impressionante)

---

## 📖 Referências e Recursos

### Documentação Oficial:
- [Actix-web Middleware](https://actix.rs/docs/middleware)
- [thiserror](https://docs.rs/thiserror/)
- [tracing](https://docs.rs/tracing/)
- [validator](https://docs.rs/validator/)

### Artigos Recomendados:
- [Error Handling in Rust](https://blog.burntsushi.net/rust-error-handling/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

---

## 🎯 Checklist de Demonstração

Quando apresentar este projeto:

- ✅ Clone o repo
- ✅ Configure `.env` com API keys
- ✅ Rode `cargo run` e mostre logs estruturados
- ✅ Teste autenticação (com/sem API key)
- ✅ Demonstre rate limiting (envie muitas requisições)
- ✅ Mostre validação de dados (envie dados inválidos)
- ✅ Rode testes com `cargo test`
- ✅ Explique arquitetura e decisões técnicas

**Tempo estimado de apresentação:** 15-20 minutos

---

## ✨ Conclusão

Esta evolução transforma o SentinelOps de uma **prova de conceito** em uma **aplicação production-ready** que demonstra:

- 🔐 Consciência de segurança
- 🧪 Disciplina de testes
- 🛠️ Tratamento robusto de erros
- 📊 Observabilidade adequada
- 💎 Domínio avançado de Rust

**Você agora tem um projeto que se destaca em entrevistas técnicas!** 🚀
