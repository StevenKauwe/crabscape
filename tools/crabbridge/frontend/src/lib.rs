#![forbid(unsafe_code)]
use axum::{
    body::Body,
    http::{header, Method, Request, Response, StatusCode},
    Router,
};
include!(concat!(env!("OUT_DIR"), "/assets.rs"));

pub fn router() -> Router {
    Router::new()
        .route("/", axum::routing::any(serve))
        .route("/{*path}", axum::routing::any(serve))
        .fallback(serve)
}

/// Mount on the existing introducer without changing its room or report routes.
pub fn mounted_router() -> Router {
    Router::new()
        .route("/crabbridge", axum::routing::any(canonical_redirect))
        .route("/crabbridge/", axum::routing::any(mounted_asset))
        .route("/crabbridge/{*path}", axum::routing::any(mounted_asset))
}

async fn mounted_asset(mut request: Request<Body>) -> Response<Body> {
    let path = request
        .uri()
        .path()
        .strip_prefix("/crabbridge")
        .unwrap_or("/missing");
    *request.uri_mut() = path
        .parse()
        .expect("A suffix of an existing URI path is valid");
    serve(request).await
}

async fn canonical_redirect(request: Request<Body>) -> Response<Body> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return serve(request).await;
    }
    let mut response = serve(
        Request::builder()
            .method(Method::HEAD)
            .uri("/")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
    response
        .headers_mut()
        .insert(header::LOCATION, "/crabbridge/".parse().unwrap());
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, "0".parse().unwrap());
    response
}

async fn serve(request: Request<Body>) -> Response<Body> {
    let path = if request.uri().path() == "/" {
        "/index.html"
    } else {
        request.uri().path()
    };
    let asset = ASSETS.iter().find(|(name, _, _)| *name == path);
    let allowed = request.method() == Method::GET || request.method() == Method::HEAD;
    let (status, mime, bytes): (_, _, &[u8]) = if !allowed {
        (
            StatusCode::METHOD_NOT_ALLOWED,
            "text/plain; charset=utf-8",
            b"Method not allowed\n",
        )
    } else if let Some((_, mime, bytes)) = asset {
        (StatusCode::OK, *mime, bytes)
    } else {
        (
            StatusCode::NOT_FOUND,
            "text/plain; charset=utf-8",
            b"Not found\n",
        )
    };
    let body = if request.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from(bytes)
    };
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-frame-options", "DENY")
        .header("referrer-policy", "no-referrer")
        .header("x-content-type-options", "nosniff")
        .header(
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        )
        .header("x-crabbridge-policy", "1")
        .header("strict-transport-security", "max-age=31536000")
        .body(body)
        .expect("Static headers are valid");
    if !allowed {
        response
            .headers_mut()
            .insert(header::ALLOW, "GET, HEAD".parse().unwrap());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use tower::ServiceExt;
    async fn request(method: Method, path: &str) -> Response<Body> {
        router()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    fn policy(response: &Response<Body>) {
        for (name, value) in [
            ("content-security-policy", CSP),
            ("cache-control", "no-store"),
            ("x-frame-options", "DENY"),
            ("referrer-policy", "no-referrer"),
            ("x-content-type-options", "nosniff"),
            (
                "permissions-policy",
                "camera=(), microphone=(), geolocation=()",
            ),
            ("x-crabbridge-policy", "1"),
        ] {
            assert_eq!(response.headers().get(name).unwrap(), value);
        }
        assert!(response.headers().get("set-cookie").is_none());
        assert!(response
            .headers()
            .get("access-control-allow-origin")
            .is_none());
    }
    #[tokio::test]
    async fn every_packaged_asset_has_exact_bytes_and_policy() {
        for (path, mime, bytes) in ASSETS {
            let response = request(Method::GET, path).await;
            assert_eq!(response.status(), StatusCode::OK);
            policy(&response);
            assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), *mime);
            assert_eq!(
                to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .as_ref(),
                *bytes
            );
            let response = request(Method::HEAD, path).await;
            policy(&response);
            assert_eq!(response.status(), StatusCode::OK);
            assert!(to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty());
        }
    }
    #[tokio::test]
    async fn no_filesystem_or_api_routes_exist() {
        for path in [
            "/config.json",
            "/invite.json",
            "/api/login",
            "/login",
            "/.git/config",
            "/../Cargo.toml",
            "/%2e%2e/Cargo.toml",
            "/vendor/../remote.js",
            "/remote.js/",
            "/remote%2ejs",
            "//remote.js",
            "/service-worker.js",
        ] {
            let response = request(Method::GET, path).await;
            policy(&response);
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
    #[tokio::test]
    async fn no_mutating_methods_or_query_reflection() {
        for method in [
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
            Method::CONNECT,
            Method::TRACE,
        ] {
            let response = request(method, "/").await;
            policy(&response);
            assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
        let response = request(Method::GET, "/?secret=never-reflect").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("never-reflect"));
    }
    #[test]
    fn frontend_build_contains_only_public_asset_allowlist() {
        assert_eq!(ASSETS.len(), 13);
        assert!(ASSETS.iter().any(|(path, _, _)| *path == "/vendor/LICENSE"));
        let html = std::str::from_utf8(
            ASSETS
                .iter()
                .find(|(p, _, _)| *p == "/index.html")
                .unwrap()
                .2,
        )
        .unwrap();
        assert!(html.contains("deployment-controls"));
        assert!(html.contains("disabled"));
    }
}
