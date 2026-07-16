use axum::routing::get;
use axum::Router;

pub fn router() -> Router {
    Router::new()
        .route("/catalog", get(catalog))
        .route("/pages", get(pages))
        .route("/filters", get(filters))
}

async fn catalog() -> &'static str {
    "catalog"
}

async fn pages() -> &'static str {
    "pages"
}

async fn filters() -> &'static str {
    "filters"
}
