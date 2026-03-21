package com.sentinelops.config;

import net.devh.boot.grpc.server.serverfactory.GrpcServerConfigurer;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

import java.util.concurrent.Executors;

/**
 * Configures the gRPC server to dispatch each incoming call to a Java 21 Virtual Thread.
 *
 * Virtual Threads (Project Loom) allow blocking I/O (DB, Kafka) inside gRPC handlers
 * without consuming OS threads. This enables the server to handle tens of thousands
 * of concurrent agent connections on modest hardware.
 *
 * Requires: Java 21 and spring.threads.virtual.enabled=true in application.yml.
 */
@Configuration
public class GrpcServerConfig {

    @Bean
    public GrpcServerConfigurer virtualThreadExecutor() {
        return serverBuilder ->
            serverBuilder.executor(Executors.newVirtualThreadPerTaskExecutor());
    }
}
