package com.sentinelops.grpc;

import io.grpc.stub.StreamObserver;
import net.devh.boot.grpc.server.service.GrpcService;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.util.UUID;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * gRPC IngestionService implementation.
 *
 * Zero Trust contract: the agent_id field in every SecurityEvent MUST match the
 * cert CN stored in the gRPC Context by AgentIdentityInterceptor. Mismatches are
 * logged and counted as rejected, never silently passed.
 */
@GrpcService
public class IngestionServiceImpl extends IngestionServiceGrpc.IngestionServiceImplBase {

    private static final Logger log = LoggerFactory.getLogger(IngestionServiceImpl.class);

    // ── Unary ─────────────────────────────────────────────────────────────────

    @Override
    public void sendEvent(SecurityEvent req, StreamObserver<EventResponse> out) {
        String certCN = AgentIdentityInterceptor.CERT_CN.get();

        // Normalise to lowercase — CERT_CN já vem lowercase do AgentIdentityInterceptor;
        // agent_id no payload pode vir em qualquer casing vindo do agente.
        if (!req.getAgentId().toLowerCase().equals(certCN)) {
            log.warn("Zero Trust violation [unary]: payload agent_id='{}' cert CN='{}'",
                     req.getAgentId(), certCN);
            out.onNext(EventResponse.newBuilder()
                .setAccepted(false)
                .setEventId(req.getEventId())
                .setMessage("agent_id mismatch — Zero Trust violation")
                .build());
            out.onCompleted();
            return;
        }

        log.info("[UNARY] type={} agent={} event={}", req.getEventType(), req.getAgentId(), req.getEventId());

        // TODO: publish to Kafka / persist to PostgreSQL

        out.onNext(EventResponse.newBuilder()
            .setAccepted(true)
            .setEventId(req.getEventId())
            .setMessage("ACK")
            .build());
        out.onCompleted();
    }

    // ── Client-side streaming ──────────────────────────────────────────────────

    @Override
    public StreamObserver<SecurityEvent> streamEvents(StreamObserver<StreamSummary> out) {
        String certCN    = AgentIdentityInterceptor.CERT_CN.get();
        String sessionId = UUID.randomUUID().toString();
        AtomicInteger accepted = new AtomicInteger(0);
        AtomicInteger rejected = new AtomicInteger(0);

        log.info("[STREAM] session={} agent(cert)={}", sessionId, certCN);

        return new StreamObserver<>() {

            @Override
            public void onNext(SecurityEvent event) {
                if (!event.getAgentId().toLowerCase().equals(certCN)) {
                    log.warn("Zero Trust violation [stream] session={}: payload agent_id='{}' cert CN='{}'",
                             sessionId, event.getAgentId(), certCN);
                    rejected.incrementAndGet();
                    return;
                }

                // TODO: publish to Kafka / persist to PostgreSQL
                accepted.incrementAndGet();
            }

            @Override
            public void onError(Throwable t) {
                log.error("[STREAM] error session={} agent={}: {}", sessionId, certCN, t.getMessage());
                // Propaga o erro para o response observer — sem isso o gRPC runtime
                // mantém o observer em estado pendente causando resource leak.
                out.onError(t);
            }

            @Override
            public void onCompleted() {
                log.info("[STREAM] closed session={} accepted={} rejected={}",
                         sessionId, accepted.get(), rejected.get());

                out.onNext(StreamSummary.newBuilder()
                    .setAcceptedCount(accepted.get())
                    .setRejectedCount(rejected.get())
                    .setSessionId(sessionId)
                    .build());
                out.onCompleted();
            }
        };
    }
}
