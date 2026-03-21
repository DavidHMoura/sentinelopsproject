package com.sentinelops.infrastructure.kafka;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.sentinelops.grpc.SecurityEvent;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.kafka.support.SendResult;

import java.util.Map;
import java.util.concurrent.CompletableFuture;

import static org.assertj.core.api.Assertions.assertThat;
import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class KafkaEventPublisherTest {

    @Mock
    private KafkaTemplate<String, String> kafkaTemplate;

    @Mock
    @SuppressWarnings("unchecked")
    private SendResult<String, String> sendResult;

    private KafkaEventPublisher publisher;
    private final ObjectMapper mapper = new ObjectMapper();

    @BeforeEach
    void setUp() {
        publisher = new KafkaEventPublisher(kafkaTemplate, "events.raw");
    }

    @Test
    void publish_sendsMessageWithAgentIdAsKeyAndCorrectJsonEnvelope() throws Exception {
        SecurityEvent event = SecurityEvent.newBuilder()
            .setEventId("evt-unit-001")
            .setEventType("auth.login.failed")
            .setSourceIp("10.0.0.1")
            .setAgentId("test-agent")
            .setSourceHost("prod-01")
            .setTimestamp("2026-03-21T17:00:00Z")
            .build();

        CompletableFuture<SendResult<String, String>> future = CompletableFuture.completedFuture(sendResult);
        when(kafkaTemplate.send(anyString(), anyString(), anyString())).thenReturn(future);

        publisher.publish(event, "test-agent");

        ArgumentCaptor<String> keyCaptor     = ArgumentCaptor.forClass(String.class);
        ArgumentCaptor<String> payloadCaptor = ArgumentCaptor.forClass(String.class);
        verify(kafkaTemplate).send(eq("events.raw"), keyCaptor.capture(), payloadCaptor.capture());
        assertThat(keyCaptor.getValue()).isEqualTo("test-agent");

        @SuppressWarnings("unchecked")
        Map<String, Object> payload = mapper.readValue(payloadCaptor.getValue(), Map.class);
        assertThat(payload.get("id")).isEqualTo("evt-unit-001");
        assertThat(payload.get("event_type")).isEqualTo("auth.login.failed");
        assertThat(payload.get("source_ip")).isEqualTo("10.0.0.1");
        assertThat(payload.get("actor")).isEqualTo("test-agent");
        assertThat(payload.get("ts")).isEqualTo("2026-03-21T17:00:00Z");

        @SuppressWarnings("unchecked")
        Map<String, Object> meta = (Map<String, Object>) payload.get("meta");
        assertThat(meta.get("source_host")).isEqualTo("prod-01");
    }

    @Test
    void publish_whenKafkaSendFails_completesWithoutThrowing() {
        SecurityEvent event = SecurityEvent.newBuilder()
            .setEventId("evt-fail-001")
            .setEventType("auth.login.failed")
            .setSourceIp("10.0.0.2")
            .setAgentId("test-agent")
            .build();

        CompletableFuture<SendResult<String, String>> failedFuture = new CompletableFuture<>();
        failedFuture.completeExceptionally(new RuntimeException("Kafka unreachable"));
        when(kafkaTemplate.send(anyString(), anyString(), anyString())).thenReturn(failedFuture);

        assertDoesNotThrow(() -> publisher.publish(event, "test-agent"));
    }

    @Test
    void publish_usesConfiguredTopicName() {
        KafkaEventPublisher customPublisher = new KafkaEventPublisher(kafkaTemplate, "events.custom");
        SecurityEvent event = SecurityEvent.newBuilder()
            .setEventId("evt-topic-001")
            .setEventType("network.scan")
            .setSourceIp("10.0.0.3")
            .setAgentId("agent-x")
            .build();

        CompletableFuture<SendResult<String, String>> future = CompletableFuture.completedFuture(sendResult);
        when(kafkaTemplate.send(anyString(), anyString(), anyString())).thenReturn(future);

        customPublisher.publish(event, "agent-x");

        verify(kafkaTemplate).send(eq("events.custom"), anyString(), anyString());
    }
}
