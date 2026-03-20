package com.sentinelops.grpc;

import io.grpc.*;
import io.grpc.inprocess.InProcessChannelBuilder;
import io.grpc.inprocess.InProcessServerBuilder;
import io.grpc.stub.StreamObserver;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.jupiter.api.Assertions.*;

// NOTE: We manage the in-process server and channel lifecycle with @BeforeEach / @AfterEach
// rather than @Rule GrpcCleanupRule (which is JUnit 4 and silently ignored by JUnit 5).
class IngestionServiceImplTest {

    private io.grpc.Server inProcessServer;
    private ManagedChannel channel;
    private IngestionServiceGrpc.IngestionServiceStub asyncStub;
    private IngestionServiceGrpc.IngestionServiceBlockingStub blockingStub;

    /** Sets up an in-process gRPC server with CERT_CN injected via Context interceptor. */
    @BeforeEach
    void setUp() throws Exception {
        String serverName = InProcessServerBuilder.generateName();
        String testCN     = "test-agent-uuid";

        // Simulate the AgentIdentityInterceptor by injecting CERT_CN into context
        ServerInterceptor cnInjector = new ServerInterceptor() {
            @Override
            public <Q, R> ServerCall.Listener<Q> interceptCall(
                ServerCall<Q, R> call, Metadata headers, ServerCallHandler<Q, R> next
            ) {
                Context ctx = Context.current().withValue(AgentIdentityInterceptor.CERT_CN, testCN);
                return Contexts.interceptCall(ctx, call, headers, next);
            }
        };

        inProcessServer = InProcessServerBuilder.forName(serverName)
            .intercept(cnInjector)
            .addService(new IngestionServiceImpl())
            .build()
            .start();

        channel = InProcessChannelBuilder.forName(serverName).directExecutor().build();

        asyncStub    = IngestionServiceGrpc.newStub(channel);
        blockingStub = IngestionServiceGrpc.newBlockingStub(channel);
    }

    @AfterEach
    void tearDown() throws InterruptedException {
        channel.shutdownNow().awaitTermination(5, TimeUnit.SECONDS);
        inProcessServer.shutdownNow().awaitTermination(5, TimeUnit.SECONDS);
    }

    // ── Unary ─────────────────────────────────────────────────────────────────

    @Test
    void sendEvent_whenAgentIdMatchesCert_returnsAccepted() {
        SecurityEvent event = SecurityEvent.newBuilder()
            .setEventId("evt-001")
            .setAgentId("test-agent-uuid")   // matches injected CERT_CN
            .setEventType("auth.login.failed")
            .setSourceIp("10.0.0.1")
            .build();

        EventResponse response = blockingStub.sendEvent(event);

        assertTrue(response.getAccepted());
        assertEquals("evt-001", response.getEventId());
    }

    @Test
    void sendEvent_whenAgentIdMismatch_returnsRejected() {
        SecurityEvent event = SecurityEvent.newBuilder()
            .setEventId("evt-002")
            .setAgentId("spoofed-agent-id")  // does NOT match CERT_CN
            .setEventType("auth.login.failed")
            .setSourceIp("10.0.0.1")
            .build();

        EventResponse response = blockingStub.sendEvent(event);

        assertFalse(response.getAccepted());
        assertTrue(response.getMessage().contains("agent_id mismatch"));
    }

    // ── Streaming ─────────────────────────────────────────────────────────────

    @Test
    void streamEvents_countsAcceptedAndRejected() throws InterruptedException {
        CountDownLatch done = new CountDownLatch(1);
        AtomicReference<StreamSummary> summaryRef = new AtomicReference<>();

        StreamObserver<SecurityEvent> requestObserver = asyncStub.streamEvents(
            new StreamObserver<StreamSummary>() {
                @Override public void onNext(StreamSummary s)      { summaryRef.set(s); }
                @Override public void onError(Throwable t)         { done.countDown(); }
                @Override public void onCompleted()                { done.countDown(); }
            }
        );

        // Send 2 valid events + 1 spoofed
        requestObserver.onNext(SecurityEvent.newBuilder()
            .setEventId("e1").setAgentId("test-agent-uuid").setEventType("network.scan").build());
        requestObserver.onNext(SecurityEvent.newBuilder()
            .setEventId("e2").setAgentId("test-agent-uuid").setEventType("auth.login.failed").build());
        requestObserver.onNext(SecurityEvent.newBuilder()
            .setEventId("e3").setAgentId("spoofed-agent").setEventType("auth.login.failed").build());
        requestObserver.onCompleted();

        assertTrue(done.await(3, TimeUnit.SECONDS), "Stream did not complete in time");

        StreamSummary summary = summaryRef.get();
        assertNotNull(summary);
        assertEquals(2, summary.getAcceptedCount());
        assertEquals(1, summary.getRejectedCount());
        assertFalse(summary.getSessionId().isEmpty());
    }
}
