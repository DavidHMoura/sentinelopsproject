package com.sentinelops.grpc;

import com.sentinelops.application.port.EventPublisher;
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
 *
 * Eventos validados são publicados via EventPublisher (port) — desacoplados
 * de qualquer detalhe de broker (Kafka, HTTP, etc.).
 */
@GrpcService
public class IngestionServiceImpl extends IngestionServiceGrpc.IngestionServiceImplBase {

    private static final Logger log = LoggerFactory.getLogger(IngestionServiceImpl.class);

    private final EventPublisher eventPublisher;

    public IngestionServiceImpl(EventPublisher eventPublisher) {
        this.eventPublisher = eventPublisher;
    }

    // ── Unary ─────────────────────────────────────────────────────────────────

    @Override
    public void sendEvent(SecurityEvent req, StreamObserver<EventResponse> out) {
        String certCN = AgentIdentityInterceptor.CERT_CN.get();

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

        eventPublisher.publish(req, certCN);

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

                eventPublisher.publish(event, certCN);
                accepted.incrementAndGet();
            }

            @Override
            public void onError(Throwable t) {
                log.error("[STREAM] error session={} agent={}: {}", sessionId, certCN, t.getMessage());
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
