use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

#[tokio::main]
async fn main() {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("file:data.db")
        .await
        .unwrap();
    // build our application with a single route
    let app = Router::new()
        .route("/key/{key}", get(handle_get_key))
        .with_state(pool.clone());

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("localhost:8000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_get_key(
    State(pool): State<SqlitePool>,
    Path(key): Path<String>,
) -> Result<String, (StatusCode, String)> {
    match insert_user(pool, &key).await {
        Ok(()) => Ok(format!("Got key {key:?}")),
        Err(e) => {
            let error_message = e.to_string();
            if error_message.contains("UNIQUE constraint failed") {
                Err((StatusCode::CONFLICT, "user already exists".to_string()))
            } else {
                Err((StatusCode::INTERNAL_SERVER_ERROR, error_message))
            }
        }
    }
}

async fn insert_user(pool: SqlitePool, fingerprint: &String) -> Result<(), sqlx::Error> {
    sqlx::query(r#"insert into users (uid) values (?)"#)
        .bind(&fingerprint)
        .execute(&pool)
        .await?;
    Ok(())
}
