package com.sentinelops.infrastructure.kafka;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.sentinelops.application.port.EventPublisher;
import com.sentinelops.grpc.SecurityEvent;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.stereotype.Component;

import java.util.HashMap;
import java.util.Map;

/**
 * Adapter de infraestrutura: publica SecurityEvent no Redpanda/Kafka
 * como um JSON envelope compatível com o modelo Event do Rust.
 *
 * Formato da mensagem:
 *   key   → certCn (agent_id normalizado, 1 partição por agente)
 *   value → JSON: { id, ts, event_type, source_ip, actor, meta }
 *
 * Falhas de publish são logadas como ERROR mas não propagadas —
 * o agente tem backoff próprio e vai retentar na próxima janela.
 *
 * NOTA: o callback .whenComplete() executa na thread do producer Kafka
 * (não na Virtual Thread do caller) — seguro para logging, não para I/O.
 */
@Component
public class KafkaEventPublisher implements EventPublisher {

    private static final Logger log = LoggerFactory.getLogger(KafkaEventPublisher.class);
    private static final ObjectMapper MAPPER = new ObjectMapper();

    private final KafkaTemplate<String, String> kafkaTemplate;
    private final String topic;

    public KafkaEventPublisher(
            KafkaTemplate<String, String> kafkaTemplate,
            @Value("${sentinelops.kafka.topic.events-raw}") String topic) {
        this.kafkaTemplate = kafkaTemplate;
        this.topic = topic;
    }

    @Override
    public void publish(SecurityEvent event, String certCn) {
        String payload;
        try {
            payload = buildJsonEnvelope(event, certCn);
        } catch (JsonProcessingException e) {
            log.error("Failed to serialize event={} agent={}: {}", event.getEventId(), certCn, e.getMessage());
            return;
        }

        kafkaTemplate.send(topic, certCn, payload)
            .whenComplete((result, ex) -> {
                if (ex != null) {
                    log.error("Kafka publish failed event={} agent={}: {}",
                              event.getEventId(), certCn, ex.getMessage());
                } else {
                    log.debug("Published event={} agent={} partition={} offset={}",
                              event.getEventId(), certCn,
                              result.getRecordMetadata().partition(),
                              result.getRecordMetadata().offset());
                }
            });
    }

    /**
     * Constrói o JSON envelope no formato esperado pelo Rust Event struct:
     *   { id, ts, event_type, source_ip, actor, meta }
     *
     * - actor          = certCn (CN validado pelo interceptor, não o agent_id do payload)
     * - meta           = { source_host, payload_encoding (se != 0), ...meta_payload JSON }
     * - ts             = string ISO-8601 do proto (campo timestamp)
     * - meta_payload   = tentativa de parse JSON; em caso de falha, meta fields omitidos
     */
    private String buildJsonEnvelope(SecurityEvent event, String certCn) throws JsonProcessingException {
        Map<String, Object> envelope = new HashMap<>();
        envelope.put("id",         event.getEventId());
        envelope.put("ts",         event.getTimestamp().isEmpty() ? null : event.getTimestamp());
        envelope.put("event_type", event.getEventType());
        envelope.put("source_ip",  event.getSourceIp());
        envelope.put("actor",      certCn);

        Map<String, Object> meta = new HashMap<>();
        meta.put("source_host", event.getSourceHost());

        // payload_encoding: indica ao consumer como interpretar meta_payload (0 = não definido)
        if (event.getPayloadEncoding() != 0) {
            meta.put("payload_encoding", event.getPayloadEncoding());
        }

        if (!event.getMetaPayload().isEmpty()) {
            try {
                @SuppressWarnings("unchecked")
                Map<String, Object> payloadMeta =
                    MAPPER.readValue(event.getMetaPayload().toByteArray(), Map.class);
                meta.putAll(payloadMeta);
            } catch (Exception e) {
                log.warn("Could not parse meta_payload for event={} — meta fields omitted",
                         event.getEventId());
            }
        }
        envelope.put("meta", meta);

        return MAPPER.writeValueAsString(envelope);
    }
}
