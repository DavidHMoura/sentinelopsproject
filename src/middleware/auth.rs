use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use std::{
    collections::HashSet,
    future::{ready, Ready},
    rc::Rc,
};

use crate::errors::SentinelError;

pub struct ApiKeyAuth {
    valid_keys: Rc<HashSet<String>>,
}

impl ApiKeyAuth {
    pub fn new(keys: Vec<String>) -> Self {
        Self {
            valid_keys: Rc::new(keys.into_iter().collect()),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for ApiKeyAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = ApiKeyAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ApiKeyAuthMiddleware {
            service: Rc::new(service),
            valid_keys: self.valid_keys.clone(),
        }))
    }
}

pub struct ApiKeyAuthMiddleware<S> {
    service: Rc<S>,
    valid_keys: Rc<HashSet<String>>,
}

impl<S, B> Service<ServiceRequest> for ApiKeyAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let valid_keys = self.valid_keys.clone();
        let service = self.service.clone();

        Box::pin(async move {
            let api_key = req
                .headers()
                .get("X-API-Key")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("");

            if !valid_keys.contains(api_key) {
                // Log apenas o IP remoto (não o valor da chave) para evitar
                // vazar tentativas de chave em logs.
                tracing::warn!(
                    remote = req.connection_info().realip_remote_addr().unwrap_or("unknown"),
                    "Authentication failed"
                );

                let error = SentinelError::AuthError("Invalid or missing API key".to_string());
                let (request, _pl) = req.into_parts();

                return Ok(ServiceResponse::new(
                    request,
                    HttpResponse::Unauthorized()
                        .json(serde_json::json!({
                            "error": error.to_string(),
                            "status": 401
                        }))
                        .map_into_right_body(),
                ));
            }

            let res = service.call(req).await?;
            Ok(res.map_into_left_body())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App, HttpResponse};

    #[actix_rt::test]
    async fn test_valid_api_key() {
        let app = test::init_service(
            App::new()
                .wrap(ApiKeyAuth::new(vec!["test-key".to_string()]))
                .route("/test", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/test")
            .insert_header(("X-API-Key", "test-key"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_rt::test]
    async fn test_invalid_api_key() {
        let app = test::init_service(
            App::new()
                .wrap(ApiKeyAuth::new(vec!["test-key".to_string()]))
                .route("/test", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/test")
            .insert_header(("X-API-Key", "wrong-key"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_rt::test]
    async fn test_missing_api_key() {
        let app = test::init_service(
            App::new()
                .wrap(ApiKeyAuth::new(vec!["test-key".to_string()]))
                .route("/test", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get().uri("/test").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }
}
