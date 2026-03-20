package com.sentinelops.grpc;

import io.grpc.*;
import net.devh.boot.grpc.server.interceptor.GrpcGlobalServerInterceptor;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import javax.net.ssl.SSLPeerUnverifiedException;
import javax.net.ssl.SSLSession;
import java.security.cert.X509Certificate;

/**
 * Zero Trust gRPC interceptor.
 *
 * Extracts the Common Name (CN) from the mTLS client certificate and stores it
 * in the gRPC Context so downstream service implementations can validate the
 * agent_id field in the SecurityEvent payload against the certified identity.
 *
 * Normalises CN to lowercase to match the AGENT_ID env var convention.
 */
@GrpcGlobalServerInterceptor
public class AgentIdentityInterceptor implements ServerInterceptor {

    private static final Logger log = LoggerFactory.getLogger(AgentIdentityInterceptor.class);

    /** gRPC Context key carrying the verified cert CN (lowercase) for service access. */
    public static final Context.Key<String> CERT_CN = Context.key("cert-cn");

    @Override
    public <Q, R> ServerCall.Listener<Q> interceptCall(
        ServerCall<Q, R> call, Metadata headers, ServerCallHandler<Q, R> next
    ) {
        String cn = extractCN(call);

        if (cn == null) {
            log.warn("gRPC call received without client certificate — rejecting (Zero Trust)");
            call.close(
                Status.UNAUTHENTICATED.withDescription("mTLS client certificate is required"),
                headers
            );
            return new ServerCall.Listener<>() {};
        }

        // Normalise to lowercase: cert CNs are case-insensitive per RFC 5280,
        // but X500Principal.getName() returns the literal string. Normalising here
        // prevents a mixed-case cert CN from failing equality checks with AGENT_ID.
        Context ctx = Context.current().withValue(CERT_CN, cn.toLowerCase());
        return Contexts.interceptCall(ctx, call, headers, next);
    }

    private <Q, R> String extractCN(ServerCall<Q, R> call) {
        SSLSession ssl = call.getAttributes().get(Grpc.TRANSPORT_ATTR_SSL_SESSION);
        if (ssl == null) return null;

        try {
            X509Certificate cert = (X509Certificate) ssl.getPeerCertificates()[0];
            String dn = cert.getSubjectX500Principal().getName();
            // DN format: "CN=value,O=org,C=BR"
            for (String part : dn.split(",")) {
                String trimmed = part.trim();
                if (trimmed.startsWith("CN=")) {
                    return trimmed.substring(3);
                }
            }
        } catch (SSLPeerUnverifiedException e) {
            log.error("Failed to verify peer certificate", e);
        } catch (ArrayIndexOutOfBoundsException e) {
            log.error("Peer certificate chain is empty", e);
        }

        return null;
    }
}
