package com.sentinelops.grpc;

import io.grpc.*;
import org.junit.jupiter.api.Test;

import javax.net.ssl.SSLPeerUnverifiedException;
import javax.net.ssl.SSLSession;
import java.security.Principal;
import java.security.cert.Certificate;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

class AgentIdentityInterceptorTest {

    private final AgentIdentityInterceptor interceptor = new AgentIdentityInterceptor();

    @Test
    void whenNoCert_callIsRejectedWithUnauthenticated() {
        @SuppressWarnings("unchecked")
        ServerCall<Object, Object> call = mock(ServerCall.class);
        Attributes attributes = Attributes.newBuilder()
            .set(Grpc.TRANSPORT_ATTR_SSL_SESSION, null)
            .build();
        when(call.getAttributes()).thenReturn(attributes);

        ServerCallHandler<Object, Object> next = mock(ServerCallHandler.class);

        interceptor.interceptCall(call, new Metadata(), next);

        verify(call).close(
            argThat(s -> s.getCode() == Status.Code.UNAUTHENTICATED),
            any(Metadata.class)
        );
        verify(next, never()).startCall(any(), any());
    }

    @Test
    void whenValidCert_cnIsNormalisedToLowercase() throws Exception {
        String mixedCaseCN = "Agent-UUID-1234";
        SSLSession sslSession = mockSslSession(mixedCaseCN);

        @SuppressWarnings("unchecked")
        ServerCall<Object, Object> call = mock(ServerCall.class);
        Attributes attributes = Attributes.newBuilder()
            .set(Grpc.TRANSPORT_ATTR_SSL_SESSION, sslSession)
            .build();
        when(call.getAttributes()).thenReturn(attributes);

        @SuppressWarnings("unchecked")
        ServerCallHandler<Object, Object> next = mock(ServerCallHandler.class);
        when(next.startCall(any(), any())).thenReturn(mock(ServerCall.Listener.class));

        final String[] capturedCN = {null};
        ServerCallHandler<Object, Object> capturingNext = (c, m) -> {
            capturedCN[0] = AgentIdentityInterceptor.CERT_CN.get();
            return mock(ServerCall.Listener.class);
        };

        interceptor.interceptCall(call, new Metadata(), capturingNext);

        assertEquals("agent-uuid-1234", capturedCN[0],
            "CN must be normalised to lowercase before storing in Context");
    }

    private SSLSession mockSslSession(String cn) throws SSLPeerUnverifiedException {
        SSLSession session = mock(SSLSession.class);
        Principal principal = mock(Principal.class);
        when(principal.getName()).thenReturn("CN=" + cn + ",O=SentinelOps,C=BR");

        java.security.cert.X509Certificate cert = mock(java.security.cert.X509Certificate.class);
        javax.security.auth.x500.X500Principal x500 = new javax.security.auth.x500.X500Principal("CN=" + cn);
        when(cert.getSubjectX500Principal()).thenReturn(x500);
        when(session.getPeerCertificates()).thenReturn(new Certificate[]{ cert });

        return session;
    }
}
