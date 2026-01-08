use axum::{Router, extract::Path, routing::get};
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::main]
async fn main() {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("file:data.db")
        .await
        .unwrap();
    // build our application with a single route
    let app = Router::new().route("/key/{key}", get(handle_get_key));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("localhost:8000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_get_key(Path(key): Path<String>) -> String {
    format!("Got key {key:?}")
}
