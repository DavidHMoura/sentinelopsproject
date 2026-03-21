package com.sentinelops.application.port;

import com.sentinelops.grpc.SecurityEvent;

/**
 * Port de saída do domínio de ingestão.
 *
 * Isola IngestionServiceImpl de qualquer detalhe de broker (Kafka, HTTP, etc.).
 * Implementações concretas vivem em com.sentinelops.infrastructure.
 *
 * Contrato: implementações devem ser thread-safe — múltiplos Virtual Threads
 * podem chamar publish() concorrentemente.
 */
public interface EventPublisher {

    /**
     * Publica o evento no canal de saída configurado.
     *
     * @param event  SecurityEvent validado pelo AgentIdentityInterceptor
     * @param certCn CN extraído do certificado mTLS (já em lowercase, já validado)
     */
    void publish(SecurityEvent event, String certCn);
}
